//! GMP integer arithmetic. `gmp_strval(gmp_add('0x100', 1))` returns `'257'`.

mod arithmetic;
mod binary_io;
mod bits;
mod functions;
mod integer;
mod number_theory;
mod object;
mod random;

use phera_extension_api::{
    ConstantValue, Extension, NativeNumericParameter, Param, ParamType, Registry,
};

static GMP_PARAMETER: NativeNumericParameter = NativeNumericParameter {
    class: "GMP",
    declaration: "GMP|string|int",
    type_error: "GMP|string|int",
    string_error: Some(integer::check),
};
const GMP_NUMBER: ParamType = ParamType::NativeNumeric(&GMP_PARAMETER);
const BASE: Param = Param::new("base", ParamType::Int).with_default(ConstantValue::Int(0));

pub struct Gmp;

impl Extension for Gmp {
    fn name(&self) -> &'static str {
        "gmp"
    }

    fn register(&self, registry: &mut Registry) {
        for (name, value) in [
            ("GMP_ROUND_ZERO", 0),
            ("GMP_ROUND_PLUSINF", 1),
            ("GMP_ROUND_MINUSINF", 2),
            ("GMP_MSW_FIRST", 1),
            ("GMP_LSW_FIRST", 2),
            ("GMP_LITTLE_ENDIAN", 4),
            ("GMP_BIG_ENDIAN", 8),
            ("GMP_NATIVE_ENDIAN", 16),
        ] {
            registry.constant(name, value);
        }
        registry.constant("GMP_VERSION", ConstantValue::String("6.3.0"));
        registry.class(object::CLASS);
        for (function, result) in functions::FUNCTIONS {
            registry.function(*function);
            registry.describe_function(function.name, result, None);
        }
    }
}
