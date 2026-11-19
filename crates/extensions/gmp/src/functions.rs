//! Native GMP signatures and conversion entry points.

use num_integer::Integer;
use num_traits::Signed;
use phera_extension_api::{ConstantValue, FunctionEntry, Param, ParamType, args};

use crate::{
    BASE, GMP_NUMBER,
    arithmetic::{self, Division},
    binary_io, bits, number_theory, random,
};

mod conversion;
use conversion::{compare, init, intval, sign, strval};

const NUM: Param = Param::new("num", GMP_NUMBER);
const ONE: &[Param] = &[NUM];
const NUM1: Param = Param::new("num1", GMP_NUMBER);
const NUM2: Param = Param::new("num2", GMP_NUMBER);
const TWO: &[Param] = &[NUM1, NUM2];
const DIVISION: &[Param] = &[
    NUM1,
    NUM2,
    Param::new("rounding_mode", ParamType::Int)
        .with_constant_default("GMP_ROUND_ZERO", ConstantValue::Int(0)),
];
const WORD_SIZE: Param =
    Param::new("word_size", ParamType::Int).with_default(ConstantValue::Int(1));
const FLAGS: Param = Param::new("flags", ParamType::Int).with_default(ConstantValue::Int(17));
const INDEX: Param = Param::new("index", ParamType::Int);
const START: Param = Param::new("start", ParamType::Int);
const ROOT: &[Param] = &[NUM, Param::new("nth", ParamType::Int)];
const OBJECT: Param = Param::new("num", ParamType::Class("GMP"));

pub(crate) const FUNCTIONS: &[(FunctionEntry, &str)] = &[
    (
        FunctionEntry::sync(
            "gmp_init",
            &[Param::new("num", ParamType::IntOrString), BASE],
            init,
        ),
        "GMP",
    ),
    (
        FunctionEntry::sync(
            "gmp_import",
            &[Param::new("data", ParamType::String), WORD_SIZE, FLAGS],
            binary_io::import,
        ),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_export", &[NUM, WORD_SIZE, FLAGS], binary_io::export),
        "string",
    ),
    (FunctionEntry::sync("gmp_intval", ONE, intval), "int"),
    (
        FunctionEntry::sync(
            "gmp_strval",
            &[
                NUM,
                Param::new("base", ParamType::Int).with_default(ConstantValue::Int(10)),
            ],
            strval,
        ),
        "string",
    ),
    (
        FunctionEntry::sync("gmp_add", TWO, |context, values| {
            arithmetic::binary(context, values, "gmp_add", |a, b| a + b)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_sub", TWO, |context, values| {
            arithmetic::binary(context, values, "gmp_sub", |a, b| a - b)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_mul", TWO, |context, values| {
            arithmetic::binary(context, values, "gmp_mul", |a, b| a * b)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_and", TWO, |context, values| {
            arithmetic::binary(context, values, "gmp_and", |a, b| a & b)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_or", TWO, |context, values| {
            arithmetic::binary(context, values, "gmp_or", |a, b| a | b)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_xor", TWO, |context, values| {
            arithmetic::binary(context, values, "gmp_xor", |a, b| a ^ b)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_gcd", TWO, |context, values| {
            arithmetic::binary(context, values, "gmp_gcd", |a, b| a.gcd(&b))
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_lcm", TWO, |context, values| {
            arithmetic::binary(context, values, "gmp_lcm", |a, b| a.lcm(&b))
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_neg", ONE, |context, values| {
            arithmetic::unary(context, values, "gmp_neg", |a| -a)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_abs", ONE, |context, values| {
            arithmetic::unary(context, values, "gmp_abs", |a| a.abs())
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_com", ONE, |context, values| {
            arithmetic::unary(context, values, "gmp_com", |a| !a)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_div", DIVISION, |context, values| {
            arithmetic::divide(context, values, "gmp_div", Division::Quotient)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_div_q", DIVISION, |context, values| {
            arithmetic::divide(context, values, "gmp_div_q", Division::Quotient)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_div_r", DIVISION, |context, values| {
            arithmetic::divide(context, values, "gmp_div_r", Division::Remainder)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_div_qr", DIVISION, |context, values| {
            arithmetic::divide(context, values, "gmp_div_qr", Division::Both)
        }),
        "array",
    ),
    (
        FunctionEntry::sync("gmp_mod", TWO, arithmetic::modulo),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_divexact", TWO, arithmetic::divexact),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_fact", ONE, arithmetic::factorial),
        "GMP",
    ),
    (
        FunctionEntry::sync(
            "gmp_pow",
            &[NUM, Param::new("exponent", ParamType::Int)],
            arithmetic::pow,
        ),
        "GMP",
    ),
    (
        FunctionEntry::sync(
            "gmp_powm",
            &[
                NUM,
                Param::new("exponent", GMP_NUMBER),
                Param::new("modulus", GMP_NUMBER),
            ],
            arithmetic::powmod,
        ),
        "GMP",
    ),
    (
        FunctionEntry::sync(
            "gmp_binomial",
            &[Param::new("n", GMP_NUMBER), Param::new("k", ParamType::Int)],
            arithmetic::binomial,
        ),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_sqrt", ONE, |context, values| {
            arithmetic::sqrt(context, values, "gmp_sqrt", false)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_sqrtrem", ONE, |context, values| {
            arithmetic::sqrt(context, values, "gmp_sqrtrem", true)
        }),
        "array",
    ),
    (
        FunctionEntry::sync("gmp_root", ROOT, |context, values| {
            arithmetic::root(context, values, "gmp_root", false)
        }),
        "GMP",
    ),
    (
        FunctionEntry::sync("gmp_rootrem", ROOT, |context, values| {
            arithmetic::root(context, values, "gmp_rootrem", true)
        }),
        "array",
    ),
    (
        FunctionEntry::sync("gmp_jacobi", TWO, |context, values| {
            number_theory::symbol(context, values, "gmp_jacobi")
        }),
        "int",
    ),
    (
        FunctionEntry::sync("gmp_legendre", TWO, |context, values| {
            number_theory::symbol(context, values, "gmp_legendre")
        }),
        "int",
    ),
    (
        FunctionEntry::sync("gmp_kronecker", TWO, |context, values| {
            number_theory::symbol(context, values, "gmp_kronecker")
        }),
        "int",
    ),
    (FunctionEntry::sync("gmp_cmp", TWO, compare), "int"),
    (FunctionEntry::sync("gmp_sign", ONE, sign), "int"),
    (
        FunctionEntry::sync("gmp_gcdext", TWO, number_theory::gcdext),
        "array",
    ),
    (
        FunctionEntry::sync("gmp_invert", TWO, number_theory::invert),
        "GMP|false",
    ),
    (
        FunctionEntry::sync("gmp_perfect_square", ONE, number_theory::square),
        "bool",
    ),
    (
        FunctionEntry::sync("gmp_perfect_power", ONE, number_theory::perfect_power),
        "bool",
    ),
    (
        FunctionEntry::sync(
            "gmp_prob_prime",
            &[
                NUM,
                Param::new("repetitions", ParamType::Int).with_default(ConstantValue::Int(10)),
            ],
            number_theory::prob_prime,
        ),
        "int",
    ),
    (
        FunctionEntry::sync("gmp_nextprime", ONE, number_theory::next_prime),
        "GMP",
    ),
    (
        FunctionEntry::sync(
            "gmp_random_seed",
            &[Param::new("seed", GMP_NUMBER)],
            random::seed,
        ),
        "void",
    ),
    (
        FunctionEntry::sync(
            "gmp_random_bits",
            &[Param::new("bits", ParamType::Int)],
            random::bits,
        ),
        "GMP",
    ),
    (
        FunctionEntry::sync(
            "gmp_random_range",
            &[Param::new("min", GMP_NUMBER), Param::new("max", GMP_NUMBER)],
            random::range,
        ),
        "GMP",
    ),
    (
        FunctionEntry::sync(
            "gmp_setbit",
            &[
                OBJECT,
                INDEX,
                Param::new("value", ParamType::Bool).with_default(ConstantValue::Bool(true)),
            ],
            |context, values| {
                bits::mutate(
                    context,
                    values,
                    "gmp_setbit",
                    args::bool(values, 2).unwrap_or(true),
                )
            },
        ),
        "void",
    ),
    (
        FunctionEntry::sync("gmp_clrbit", &[OBJECT, INDEX], |context, values| {
            bits::mutate(context, values, "gmp_clrbit", false)
        }),
        "void",
    ),
    (
        FunctionEntry::sync("gmp_testbit", &[NUM, INDEX], bits::test),
        "bool",
    ),
    (
        FunctionEntry::sync("gmp_scan0", &[NUM1, START], |context, values| {
            bits::scan(context, values, "gmp_scan0", false)
        }),
        "int",
    ),
    (
        FunctionEntry::sync("gmp_scan1", &[NUM1, START], |context, values| {
            bits::scan(context, values, "gmp_scan1", true)
        }),
        "int",
    ),
    (
        FunctionEntry::sync("gmp_popcount", ONE, bits::popcount),
        "int",
    ),
    (
        FunctionEntry::sync("gmp_hamdist", TWO, bits::distance),
        "int",
    ),
];
