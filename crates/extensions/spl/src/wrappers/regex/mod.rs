//! RegexIterator filters inner rows and can replace a row with matching captures.
//!
//! ```php
//! $matches = new RegexIterator($files, '/([a-z]+)\\.php/', RegexIterator::GET_MATCH);
//! ```

mod matching;
mod registration;

pub(in crate::wrappers) use registration::register;

use phera_extension_api::{
    Context, LocalFuture, NativeError, NativeResult, Object, PhpString, Value, args,
};
use std::rc::Rc;

use super::{Wrapper, new_wrapper, read, update};
use crate::{This, receiver};

const CLASS: &str = "RegexIterator";
const RECURSIVE: &str = "RecursiveRegexIterator";
const MODES: &str = "must be RegexIterator::MATCH, RegexIterator::GET_MATCH, \
    RegexIterator::ALL_MATCHES, RegexIterator::SPLIT, or RegexIterator::REPLACE";

#[derive(Clone, Copy)]
enum Mode {
    Match,
    GetMatch,
    AllMatches,
    Split,
    Replace,
}

impl Mode {
    fn parse(raw: i64, class: &str, method: &str, argument: usize) -> Result<Self, NativeError> {
        match raw {
            0 => Ok(Self::Match),
            1 => Ok(Self::GetMatch),
            2 => Ok(Self::AllMatches),
            3 => Ok(Self::Split),
            4 => Ok(Self::Replace),
            _ => Err(NativeError::value_error(format!(
                "{class}::{method}(): Argument #{argument} ($mode) {MODES}"
            ))),
        }
    }

    fn raw(self) -> i64 {
        match self {
            Self::Match => 0,
            Self::GetMatch => 1,
            Self::AllMatches => 2,
            Self::Split => 3,
            Self::Replace => 4,
        }
    }
}

#[derive(Clone)]
pub(in crate::wrappers) struct Regex {
    pattern: PhpString,
    mode: Mode,
    flags: i64,
    preg_flags: i64,
}

fn get<T>(object: &Object, f: impl FnOnce(&Regex) -> T) -> Result<T, NativeError> {
    read(object, |state| state.regex.as_ref().map(f))?.ok_or_else(super::uninitialized)
}

fn change<T>(object: &Object, f: impl FnOnce(&mut Regex) -> T) -> Result<T, NativeError> {
    update(object, |state| state.regex.as_mut().map(f))?.ok_or_else(super::uninitialized)
}

fn construct(
    context: &mut dyn Context,
    this: Option<Rc<Object>>,
    arguments: Vec<Value>,
) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let object = receiver(this.as_ref())?;
        let class = if object.class.is_instance_of(RECURSIVE) {
            RECURSIVE
        } else {
            CLASS
        };
        let pattern = args::string(&arguments, 1).ok_or_else(super::uninitialized)?;
        let mode = Mode::parse(
            args::int(&arguments, 2).unwrap_or_default(),
            class,
            "__construct",
            3,
        )?;
        phera_ext_pcre::validate_pattern(pattern.as_bytes()).map_err(|message| {
            NativeError::throw(
                "InvalidArgumentException",
                format!("{class}::__construct(): {message}"),
            )
        })?;
        let regex = Regex {
            pattern: pattern.clone(),
            mode,
            flags: args::int(&arguments, 3).unwrap_or_default(),
            preg_flags: args::int(&arguments, 4).unwrap_or_default(),
        };
        let inner = args::object(&arguments, 0)
            .ok_or_else(super::uninitialized)?
            .clone();
        new_wrapper(
            context,
            object,
            inner,
            Wrapper {
                regex: Some(regex),
                ..Wrapper::default()
            },
        )
        .await
    })
}

fn get_regex(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |regex| {
        Value::String(regex.pattern.clone())
    })
}

fn get_mode(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |regex| Value::Int(regex.mode.raw()))
}

fn set_mode(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    let mode = Mode::parse(
        args::int(arguments, 0).unwrap_or_default(),
        CLASS,
        "setMode",
        1,
    )?;
    change(receiver(this)?, |regex| {
        regex.mode = mode;
        Value::Null
    })
}

fn get_flags(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |regex| Value::Int(regex.flags))
}

fn set_flags(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    change(receiver(this)?, |regex| {
        regex.flags = args::int(arguments, 0).unwrap_or_default();
        Value::Null
    })
}

fn get_preg_flags(_: &mut dyn Context, this: This<'_>, _: &[Value]) -> NativeResult {
    get(receiver(this)?, |regex| Value::Int(regex.preg_flags))
}

fn set_preg_flags(_: &mut dyn Context, this: This<'_>, arguments: &[Value]) -> NativeResult {
    change(receiver(this)?, |regex| {
        regex.preg_flags = args::int(arguments, 0).unwrap_or_default();
        Value::Null
    })
}
