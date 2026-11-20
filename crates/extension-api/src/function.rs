//! Native parameters keep PHP declarations such as `str_pad($string, $length)`.

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
    DeprecatedSync(SyncFn),
    Async(AsyncFn),
}

/// Emit an installed method's catalog notice before its argument binder runs.
pub fn deprecated_method(
    context: &mut dyn Context,
    name: &str,
) -> Result<crate::MethodPreparation, crate::NativeError> {
    use phera_source::deprecations::{ItemKind, kept_notice};
    if let Some(message) = kept_notice(ItemKind::Method, name) {
        context.deprecated(message.into());
    }
    Ok(crate::MethodPreparation::Default)
}

/// The declared type of a parameter, used to coerce and check arguments before
/// the function runs, with PHP's rules and PHP's error messages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    /// A parameter with no declaration. It accepts the same values as `mixed`.
    Untyped,
    Mixed,
    Bool,
    Int,
    Float,
    String,
    Array,
    Object,
    /// Printed as object, checked against the reflected class by the method.
    ReflectedObject,
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
    /// A native numeric object or one of its scalar inputs.
    NativeNumeric(&'static NativeNumericParameter),
    /// `array|int`
    ArrayOrInt,
    /// `array|string`
    ArrayOrString,
    /// `object|string`
    ObjectOrString,
    /// `OpenSSLCertificate|string`
    CertificateOrString,
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
    /// A PHP declaration with named classes or interfaces and an optional
    /// string arm. Nullability is supplied separately by Param.
    ClassUnion(&'static str),
}

/// Early validation for a native numeric type's string input.
pub type NumericStringError = fn(&[u8]) -> Option<&'static str>;

/// Native numeric types can reject malformed strings while preparing a call.
/// This preserves the order of validation and coercion for later arguments.
#[derive(Debug)]
pub struct NativeNumericParameter {
    pub class: &'static str,
    pub declaration: &'static str,
    pub type_error: &'static str,
    pub string_error: Option<NumericStringError>,
}

impl PartialEq for NativeNumericParameter {
    fn eq(&self, other: &Self) -> bool {
        std::ptr::eq(self, other)
    }
}

impl Eq for NativeNumericParameter {}

impl ParamType {
    pub fn name(self) -> &'static str {
        crate::arguments::type_text(self)
    }

    pub fn union_accepts_class(self, class: &dyn phera_value::ClassHandle) -> bool {
        match self {
            Self::ClassUnion(declaration) => declaration
                .split('|')
                .filter(|name| *name != "string")
                .any(|name| class.is_instance_of(name)),
            _ => false,
        }
    }

    pub fn union_accepts_string(self) -> bool {
        matches!(self, Self::ClassUnion(declaration) if declaration.split('|').any(|member| member == "string"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Param {
    /// Without the `$`. Used for named arguments and in error messages.
    pub name: &'static str,
    pub ty: ParamType,
    pub nullable: bool,
    null_coercion: NullCoercion,
    arity: ArgumentCount,
    pub rejects_nul: bool,
    pub optional: bool,
    /// Fills a skipped position when a later named argument is supplied.
    pub default: Option<ConstantValue>,
    /// The constant spelling PHP uses in a declaration, when the value is symbolic.
    pub default_name: Option<&'static str>,
    pub by_ref: bool,
    reference_mode: ReferenceMode,
    pub(crate) raw_reference: bool,
    /// Replace the argument value in PHP traces.
    pub sensitive: bool,
    pub variadic: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NullCoercion {
    Declared,
    Scalar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArgumentCount {
    Parameters,
    Handler,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReferenceMode {
    Required,
    Preferred,
}

impl Param {
    pub const fn new(name: &'static str, ty: ParamType) -> Self {
        Self {
            name,
            ty,
            nullable: false,
            null_coercion: NullCoercion::Declared,
            arity: ArgumentCount::Parameters,
            rejects_nul: false,
            optional: false,
            default: None,
            default_name: None,
            by_ref: false,
            reference_mode: ReferenceMode::Required,
            raw_reference: false,
            sensitive: false,
            variadic: false,
        }
    }

    /// PHP validates NUL bytes in paths before parsing the next argument.
    pub const fn without_nul(mut self) -> Self {
        self.rejects_nul = true;
        self
    }

    pub const fn nullable(mut self) -> Self {
        self.nullable = true;
        self
    }

    /// A native parser may coerce an explicit null despite a nullable declaration.
    /// For example, `mb_encode_mimeheader($text, null)` coerces null to an empty charset.
    pub const fn coerce_nullable_scalar(mut self) -> Self {
        self.nullable = true;
        self.null_coercion = NullCoercion::Scalar;
        self
    }

    pub const fn accepts_null(self) -> bool {
        self.nullable && matches!(self.null_coercion, NullCoercion::Declared)
    }

    /// On the first parameter, preserve a handler's own argument count checks.
    ///
    /// Some signatures have multiple call forms, such as `mt_rand()` and
    /// `mt_rand($min, $max)`. The reflected parameters describe both forms.
    pub const fn arity_checked_by_handler(mut self) -> Self {
        self.arity = ArgumentCount::Handler;
        self
    }

    pub(crate) const fn uses_parameter_arity(&self) -> bool {
        matches!(self.arity, ArgumentCount::Parameters)
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
        self.default_name = None;
        self
    }

    pub const fn with_constant_default(
        mut self,
        name: &'static str,
        default: ConstantValue,
    ) -> Self {
        self = self.with_default(default);
        self.default_name = Some(name);
        self
    }

    pub const fn by_ref(mut self) -> Self {
        self.by_ref = true;
        self.reference_mode = ReferenceMode::Required;
        self
    }

    /// Bind a writable argument, but accept a temporary by value.
    pub const fn prefer_ref(mut self) -> Self {
        self.by_ref = true;
        self.reference_mode = ReferenceMode::Preferred;
        self
    }

    pub const fn requires_ref(self) -> bool {
        self.by_ref && matches!(self.reference_mode, ReferenceMode::Required)
    }

    /// Preserve the declared type for reflection and let the native function check
    /// the original referenced value and report its own diagnostic.
    pub const fn raw_by_ref(mut self) -> Self {
        self = self.by_ref();
        self.raw_reference = true;
        self
    }

    pub const fn sensitive(mut self) -> Self {
        self.sensitive = true;
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

    /// A retained deprecated function dispatches its notice before argument parsing.
    pub const fn deprecated_sync(
        name: &'static str,
        params: &'static [Param],
        handler: SyncFn,
    ) -> Self {
        Self {
            name,
            params,
            handler: NativeFn::DeprecatedSync(handler),
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
