//! `ob_get_status(true)` reports active buffers from outermost to innermost.

use phera_extension_api::{
    Context, NativeError, NativeResult, Param, ParamType, args, prepare_call,
};
use phera_value::{PhpArray, Value};

use crate::Vm;

use super::{Buffer, DEFAULT_NAME, Operation, STDFLAGS};

#[derive(Clone, Copy)]
pub(crate) enum OutputFunction {
    Start,
    Contents,
    GetClean,
    GetFlush,
    Length,
    Level,
    Status,
    ListHandlers,
    Clean,
    FlushBuffer,
    EndClean,
    EndFlush,
    ImplicitFlush,
    Flush,
}

impl OutputFunction {
    pub(crate) const ALL: &[Self] = &[
        Self::Start,
        Self::Contents,
        Self::GetClean,
        Self::GetFlush,
        Self::Length,
        Self::Level,
        Self::Status,
        Self::ListHandlers,
        Self::Clean,
        Self::FlushBuffer,
        Self::EndClean,
        Self::EndFlush,
        Self::ImplicitFlush,
        Self::Flush,
    ];

    pub(crate) fn resolve(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|function| function.name() == name)
    }

    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Start => "ob_start",
            Self::Contents => "ob_get_contents",
            Self::GetClean => "ob_get_clean",
            Self::GetFlush => "ob_get_flush",
            Self::Length => "ob_get_length",
            Self::Level => "ob_get_level",
            Self::Status => "ob_get_status",
            Self::ListHandlers => "ob_list_handlers",
            Self::Clean => "ob_clean",
            Self::FlushBuffer => "ob_flush",
            Self::EndClean => "ob_end_clean",
            Self::EndFlush => "ob_end_flush",
            Self::ImplicitFlush => "ob_implicit_flush",
            Self::Flush => "flush",
        }
    }

    pub(crate) const fn parameters(self) -> &'static [Param] {
        const START: &[Param] = &[
            Param::new("callback", ParamType::Mixed)
                .nullable()
                .optional(),
            Param::new("chunk_size", ParamType::Int).optional(),
            Param::new("flags", ParamType::Int).optional(),
        ];
        const STATUS: &[Param] = &[Param::new("full_status", ParamType::Bool).optional()];
        const IMPLICIT: &[Param] = &[Param::new("enable", ParamType::Bool).optional()];

        match self {
            Self::Start => START,
            Self::Status => STATUS,
            Self::ImplicitFlush => IMPLICIT,
            _ => &[],
        }
    }
}

impl Vm {
    pub(crate) async fn output_function(
        &mut self,
        function: OutputFunction,
        mut arguments: Vec<Value>,
    ) -> NativeResult {
        prepare_call(function.name(), function.parameters(), &mut arguments, self)?;
        let stack = &self.output_writer.buffers.stack;
        let operation = match function {
            OutputFunction::Start => return self.start_buffer(&arguments).await,
            OutputFunction::Contents => {
                return Ok(stack.last().map_or(Value::Bool(false), |buffer| {
                    Value::from(buffer.bytes.as_slice())
                }));
            }
            OutputFunction::Length => {
                return Ok(stack.last().map_or(Value::Bool(false), |buffer| {
                    Value::Int(buffer.bytes.len() as i64)
                }));
            }
            OutputFunction::Level => return Ok(Value::Int(stack.len() as i64)),
            OutputFunction::Status => {
                if !args::bool(&arguments, 0).unwrap_or(false)
                    && let Some(buffer) = stack.last()
                {
                    return Ok(buffer.status(stack.len() - 1));
                }
                let mut statuses = PhpArray::new();
                for (level, buffer) in stack.iter().enumerate() {
                    statuses.push(buffer.status(level))?;
                }
                return Ok(Value::from(statuses));
            }
            OutputFunction::ListHandlers => {
                let mut names = PhpArray::new();
                for buffer in stack {
                    names.push(Value::from(buffer.name.as_str()))?;
                }
                return Ok(Value::from(names));
            }
            OutputFunction::ImplicitFlush => {
                self.output_writer
                    .set_implicit_flush(args::bool(&arguments, 0).unwrap_or(true));
                return Ok(Value::Null);
            }
            OutputFunction::Flush => {
                self.flush_transport().await?;
                return Ok(Value::Null);
            }
            OutputFunction::GetClean => Operation::GetClean,
            OutputFunction::GetFlush => Operation::GetFlush,
            OutputFunction::Clean => Operation::Clean,
            OutputFunction::FlushBuffer => Operation::Flush,
            OutputFunction::EndClean => Operation::EndClean,
            OutputFunction::EndFlush => Operation::EndFlush,
        };
        self.change_buffer(function, operation).await
    }

    async fn start_buffer(&mut self, arguments: &[Value]) -> NativeResult {
        self.check_output_handler("ob_start")?;
        let callable = args::value(arguments, 0);
        let (name, callback) = if callable.is_null() {
            (DEFAULT_NAME.to_string(), None)
        } else {
            match self.request_callable("ob_start", callable).await {
                Ok(callee) => (handler_name(callable, &callee.name), Some(callee)),
                Err(NativeError::Throw { message, .. }) => {
                    let detail = message
                        .split_once("valid callback, ")
                        .map_or(message.as_str(), |(_, detail)| detail);
                    self.warn(format!("ob_start(): {detail}"));
                    self.notice("ob_start(): Failed to create buffer".into());
                    return Ok(Value::Bool(false));
                }
                Err(error) => return Err(error),
            }
        };
        let chunk = args::int(arguments, 1).unwrap_or(0).max(0) as usize;
        let flags = args::int(arguments, 2).unwrap_or(STDFLAGS);
        self.output_writer
            .buffers
            .stack
            .push(Buffer::new(name, callback, chunk, flags));
        Ok(Value::Bool(true))
    }
}

fn handler_name(value: &Value, resolved: &str) -> String {
    match &*value.deref() {
        Value::String(name) => name.to_string_lossy(),
        Value::Object(object) if object.class.name() != "Closure" => {
            format!("{}::__invoke", object.class.name())
        }
        _ => resolved.to_owned(),
    }
}
