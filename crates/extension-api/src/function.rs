use phera_value::Value;

use crate::{ConstantValue, Context, LocalFuture, NativeResult};

/// A native function that finishes without waiting for anything.
///
/// By-reference parameters arrive as [`Value::Reference`]; the function writes
/// through the reference to change the caller's variable.
pub type SyncFn = fn(&mut dyn Context, &[Value]) -> NativeResult;

/// Select a synchronous body for already typed arguments. For example,
/// `str_replace('a', 'b', $text)` cannot call PHP when all three are strings.
pub type SyncSelector = fn(&[Option<Value>]) -> Option<SyncFn>;

/// A native function that may wait: for I/O, a timer, or a PHP callback. The
/// interpreter awaits it, which lets other requests run on the same thread
/// meanwhile. It must never block the thread instead.
pub type AsyncFn = for<'a> fn(&'a mut dyn Context, Vec<Value>) -> LocalFuture<'a, NativeResult>;

#[derive(Clone, Copy)]
pub enum NativeFn {
    Sync(SyncFn),
    Async(AsyncFn),
}

/// The declared type of a parameter, used to coerce and check arguments before
/// the function runs, with PHP's rules and PHP's error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    Mixed,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    /// `object|array`
    ObjectOrArray,
    Class(&'static str),
    /// `RoundingMode|int`
    RoundingModeOrInt,
    Callable,
    Iterable,
    /// `int|float`
    Number,
    /// `int|string`
    IntOrString,
    /// `array|int`
    ArrayOrInt,
    /// `array|string`
    ArrayOrString,
    /// `object|string`
    ObjectOrString,
    /// `Closure|string`
    ClosureOrString,
    /// `string|int|float`, used by phpredis blocking pop timeouts.
    StringOrNumber,
    /// `array|int|float`, used by geographic search shapes.
    ArrayOrNumber,
    /// `array|float`, used by sorted-set scores or options.
    ArrayOrFloat,
    /// `array|bool`, used by sorted-set range options.
    ArrayOrBool,
    /// `array|object`
    ArrayOrObject,
    /// `RecursiveIterator|IteratorAggregate`
    RecursiveOrAggregate,
}

impl ParamType {
    pub fn name(self) -> &'static str {
        crate::arguments::type_text(self)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    /// Without the `$`. Used for named arguments and in error messages.
    pub name: &'static str,
    pub ty: ParamType,
    pub nullable: bool,
    pub optional: bool,
    /// Fills a skipped position when a later named argument is supplied.
    pub default: Option<ConstantValue>,
    pub by_ref: bool,
    pub variadic: bool,
}

impl Param {
    pub const fn new(name: &'static str, ty: ParamType) -> Self {
        Self {
            name,
            ty,
            nullable: false,
            optional: false,
            default: None,
            by_ref: false,
            variadic: false,
        }
    }

    pub const fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    /// The parameter has a default; the function sees a shorter argument slice
    /// when it is left out.
    pub const fn optional(mut self) -> Self {
        self.optional = true;
        self
    }

    pub const fn with_default(mut self, default: ConstantValue) -> Self {
        self.optional = true;
        self.default = Some(default);
        self
    }

    pub const fn by_ref(mut self) -> Self {
        self.by_ref = true;
        self
    }

    /// Collects all remaining arguments. Must be the last parameter.
    pub const fn variadic(mut self) -> Self {
        self.variadic = true;
        self.optional = true;
        self
    }
}

/// A native function together with its signature.
#[derive(Clone, Copy)]
pub struct FunctionEntry {
    pub name: &'static str,
    pub params: &'static [Param],
    pub handler: NativeFn,
}

impl FunctionEntry {
    pub const fn sync(name: &'static str, params: &'static [Param], handler: SyncFn) -> Self {
        Self {
            name,
            params,
            handler: NativeFn::Sync(handler),
        }
    }

    pub const fn asynchronous(
        name: &'static str,
        params: &'static [Param],
        handler: AsyncFn,
    ) -> Self {
        Self {
            name,
            params,
            handler: NativeFn::Async(handler),
        }
    }

    /// How many arguments a call must pass at least.
    pub fn min_args(&self) -> usize {
        self.params.iter().filter(|param| !param.optional).count()
    }

    /// How many arguments a call may pass at most; `None` if variadic.
    pub fn max_args(&self) -> Option<usize> {
        match self.params.last() {
            Some(last) if last.variadic => None,
            _ => Some(self.params.len()),
        }
    }
}
