//! `curl_multi_exec($multi, $running)` advances multiple independent HTTP transfers.

mod work;

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use phera_extension_api::{
    ArrayKey, ConstantValue, Context, FunctionEntry, NativeError, NativeResult, Object, Param,
    ParamType, PhpArray, Registry, Value, args,
};

use crate::{classes, constants::ids::*, easy, options::Output, transfer::Transfer};

type Handle = Rc<RefCell<Multi>>;

#[derive(Default)]
pub(crate) struct Multi {
    entries: Vec<Entry>,
    messages: VecDeque<(Rc<Object>, i64)>,
    error: i64,
    max_host: usize,
    max_total: usize,
    max_cached: usize,
    multiplex: bool,
    executing: bool,
}

impl Multi {
    /// The copy for a fork of a snapshot, while no transfer was added.
    pub(crate) fn forked(
        &self,
        fork: &mut phera_extension_api::value::Fork<'_>,
    ) -> Result<Self, String> {
        if !self.entries.is_empty() || self.executing {
            return Err("handles are added to it; add them after Phera\\snapshot()".into());
        }
        Ok(Multi {
            entries: Vec::new(),
            messages: self
                .messages
                .iter()
                .map(|(object, code)| (fork.object(object), *code))
                .collect(),
            error: self.error,
            max_host: self.max_host,
            max_total: self.max_total,
            max_cached: self.max_cached,
            multiplex: self.multiplex,
            executing: false,
        })
    }
}

struct Entry {
    object: Rc<Object>,
    handle: easy::Handle,
    work: Work,
}

enum Work {
    New,
    Running(Box<Transfer>),
    Stepping,
    Done,
}

const HANDLE: Param = Param::new("multi_handle", ParamType::Class("CurlMultiHandle"));
const RECURSIVE: i64 = 8;

pub(crate) fn register(registry: &mut Registry) {
    const FUNCTIONS: &[FunctionEntry] = &[
        FunctionEntry::sync("curl_multi_init", &[], init),
        FunctionEntry::sync("curl_multi_add_handle", &[HANDLE, easy::HANDLE], add),
        FunctionEntry::sync("curl_multi_remove_handle", &[HANDLE, easy::HANDLE], remove),
        FunctionEntry::sync("curl_multi_get_handles", &[HANDLE], get_handles),
        FunctionEntry::asynchronous(
            "curl_multi_exec",
            &[
                HANDLE,
                Param::new("still_running", ParamType::Mixed).by_ref(),
            ],
            work::exec,
        ),
        FunctionEntry::asynchronous(
            "curl_multi_select",
            &[
                HANDLE,
                Param::new("timeout", ParamType::Float).with_default(ConstantValue::Float(1.0)),
            ],
            work::select,
        ),
        FunctionEntry::sync(
            "curl_multi_info_read",
            &[
                HANDLE,
                Param::new("queued_messages", ParamType::Mixed)
                    .by_ref()
                    .with_default(ConstantValue::Null),
            ],
            info_read,
        ),
        FunctionEntry::sync("curl_multi_getcontent", &[easy::HANDLE], getcontent),
        FunctionEntry::sync(
            "curl_multi_setopt",
            &[
                HANDLE,
                Param::new("option", ParamType::Int),
                Param::new("value", ParamType::Mixed),
            ],
            setopt,
        ),
        FunctionEntry::sync("curl_multi_errno", &[HANDLE], errno),
        FunctionEntry::sync(
            "curl_multi_strerror",
            &[easy::ERROR],
            crate::error::curl_multi_strerror,
        ),
        FunctionEntry::sync("curl_multi_close", &[HANDLE], close),
    ];
    for &entry in FUNCTIONS {
        registry.function(entry);
    }
}

fn init(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    classes::create(
        context,
        "CurlMultiHandle",
        Rc::new(RefCell::new(Multi {
            multiplex: true,
            ..Multi::default()
        })),
    )
}

fn handle(values: &[Value]) -> Result<Handle, NativeError> {
    classes::state(easy::object(values, 0)?)
}

fn add(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let multi = handle(values)?;
    let object = easy::object(values, 1)?.clone();
    let handle: easy::Handle = classes::state(&object)?;
    let mut state = multi.borrow_mut();
    let attached = {
        let easy = handle.borrow();
        easy.active
            || easy
                .multi
                .as_ref()
                .and_then(|owner| owner.upgrade())
                .is_some()
    };
    let error = if state.executing {
        RECURSIVE
    } else if attached {
        7
    } else {
        let mut easy = handle.borrow_mut();
        easy.settings
            .discard_closed_streams(context, "curl_multi_add_handle");
        easy.multi = Some(Rc::downgrade(&multi));
        drop(easy);
        state.entries.push(Entry {
            object,
            handle,
            work: Work::New,
        });
        0
    };
    state.error = error;
    Ok(Value::Int(error))
}

fn remove(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let multi = handle(values)?;
    let object = easy::object(values, 1)?;
    let mut state = multi.borrow_mut();
    if state.executing {
        state.error = RECURSIVE;
        return Ok(Value::Int(RECURSIVE));
    }
    if let Some(index) = state
        .entries
        .iter()
        .position(|entry| Rc::ptr_eq(&entry.object, object))
    {
        let entry = state.entries.remove(index);
        entry.handle.borrow_mut().multi = None;
        state
            .messages
            .retain(|(candidate, _)| !Rc::ptr_eq(candidate, object));
    }
    state.error = 0;
    Ok(Value::Int(0))
}

fn get_handles(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let mut list = PhpArray::new();
    for entry in &handle(values)?.borrow().entries {
        list.push(Value::Object(entry.object.clone()))?;
    }
    Ok(Value::from(list))
}

fn getcontent(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let handle = easy::handle(values)?;
    let easy = handle.borrow();
    if matches!(easy.settings.output, Output::Return) {
        Ok(Value::from(easy.content.clone().unwrap_or_default()))
    } else {
        Ok(Value::Null)
    }
}

fn info_read(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let handle = handle(values)?;
    let mut multi = handle.borrow_mut();
    let queue = multi.messages.pop_front();
    let Some((object, result)) = queue else {
        return Ok(Value::Bool(false));
    };
    set_reference(values, multi.messages.len() as i64)?;
    let mut message = PhpArray::new();
    message.insert(ArrayKey::str("msg"), Value::Int(CURLMSG_DONE));
    message.insert(ArrayKey::str("result"), Value::Int(result));
    message.insert(ArrayKey::str("handle"), Value::Object(object));
    Ok(Value::from(message))
}

fn setopt(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let handle = handle(values)?;
    let mut multi = handle.borrow_mut();
    let option = args::int(values, 1).unwrap_or_default();
    let value = args::value(values, 2);
    let number = || usize::try_from(option_number(value)).unwrap_or(0);
    match option {
        CURLMOPT_MAX_HOST_CONNECTIONS => multi.max_host = number(),
        CURLMOPT_MAX_TOTAL_CONNECTIONS => multi.max_total = number(),
        CURLMOPT_MAXCONNECTS => multi.max_cached = number(),
        CURLMOPT_PIPELINING => multi.multiplex = option_number(value) & CURLPIPE_MULTIPLEX != 0,
        CURLMOPT_PUSHFUNCTION
        | CURLMOPT_MAX_CONCURRENT_STREAMS
        | CURLMOPT_MAX_PIPELINE_LENGTH
        | CURLMOPT_CONTENT_LENGTH_PENALTY_SIZE
        | CURLMOPT_CHUNK_LENGTH_PENALTY_SIZE => {
            multi.error = 6;
            context.warning(
                "curl_multi_setopt(): option is unavailable in Phera's HTTP client".into(),
            );
            return Ok(Value::Bool(false));
        }
        _ => {
            multi.error = 6;
            return Err(NativeError::value_error(
                "curl_multi_setopt(): Argument #2 ($option) is not a valid cURL multi option",
            ));
        }
    }
    multi.error = 0;
    Ok(Value::Bool(true))
}

fn option_number(value: &Value) -> i64 {
    match &*value.deref() {
        Value::Int(number) => *number,
        Value::Bool(flag) => i64::from(*flag),
        Value::Float(float) => *float as i64,
        Value::InternedStr(string) => String::from_utf8_lossy(string.as_bytes())
            .parse()
            .unwrap_or(0),
        Value::String(string) => String::from_utf8_lossy(string.as_bytes())
            .parse()
            .unwrap_or(0),
        _ => 0,
    }
}

fn errno(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    Ok(Value::Int(handle(values)?.borrow().error))
}

fn close(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let handle = handle(values)?;
    let mut multi = handle.borrow_mut();
    if multi.executing {
        return Err(NativeError::error(
            "curl_multi_close(): Attempt to close cURL multi handle from a callback",
        ));
    }
    for entry in multi.entries.drain(..) {
        entry.handle.borrow_mut().multi = None;
    }
    multi.messages.clear();
    multi.error = 0;
    Ok(Value::Null)
}

fn set_reference(values: &[Value], int: i64) -> Result<(), NativeError> {
    if let Some(reference) = args::reference(values, 1) {
        *reference
            .try_borrow_mut()
            .map_err(|_| NativeError::error("cURL result is already borrowed"))? = Value::Int(int);
    }
    Ok(())
}
