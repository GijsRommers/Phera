//! `gmp_export(256, 2, GMP_BIG_ENDIAN)` writes one two-byte word.

use num_bigint::{BigInt, Sign};
use num_traits::Zero;
use phera_extension_api::{Context, NativeResult, Value, args};

use crate::{
    integer::{self, Argument, Result},
    object,
};

#[derive(Clone, Copy)]
enum Order {
    MostSignificant,
    LeastSignificant,
}

#[derive(Clone, Copy)]
enum Endian {
    Big,
    Little,
}

struct Layout {
    size: usize,
    order: Order,
    endian: Endian,
}

impl Layout {
    fn parse(function: &'static str, values: &[Value]) -> Result<Self> {
        let size = args::int(values, 1).unwrap_or(1);
        let flags = args::int(values, 2).unwrap_or(17);
        if size < 1 {
            return Err(Argument(function, 2, "word_size")
                .value_error("must be greater than or equal to 1"));
        }
        let argument = Argument(function, 3, "flags");
        let order = match flags & 3 {
            0 | 1 => Order::MostSignificant,
            2 => Order::LeastSignificant,
            _ => return Err(argument.value_error("cannot use multiple word order options")),
        };
        let endian = match flags & 28 {
            4 => Endian::Little,
            8 => Endian::Big,
            0 | 16 => {
                if cfg!(target_endian = "little") {
                    Endian::Little
                } else {
                    Endian::Big
                }
            }
            _ => return Err(argument.value_error("cannot use multiple endian options")),
        };
        Ok(Self {
            size: size as usize,
            order,
            endian,
        })
    }

    fn permute(&self, bytes: &mut [u8]) {
        match (self.order, self.endian) {
            (Order::MostSignificant, Endian::Big) => {}
            (Order::LeastSignificant, Endian::Little) => bytes.reverse(),
            (Order::MostSignificant, Endian::Little) => {
                for word in bytes.chunks_exact_mut(self.size) {
                    word.reverse();
                }
            }
            (Order::LeastSignificant, Endian::Big) => {
                bytes.reverse();
                for word in bytes.chunks_exact_mut(self.size) {
                    word.reverse();
                }
            }
        }
    }
}

pub(crate) fn import(context: &mut dyn Context, values: &[Value]) -> NativeResult {
    let layout = Layout::parse("gmp_import", values)?;
    let mut bytes = args::bytes(values, 0).to_vec();
    if !bytes.len().is_multiple_of(layout.size) {
        return Err(Argument("gmp_import", 1, "data")
            .value_error("must be a multiple of argument #2 ($word_size)"));
    }
    layout.permute(&mut bytes);
    object::create(context, BigInt::from_bytes_be(Sign::Plus, &bytes))
}

pub(crate) fn export(_: &mut dyn Context, values: &[Value]) -> NativeResult {
    let number = integer::unary("gmp_export", values)?;
    let layout = Layout::parse("gmp_export", values)?;
    if number.is_zero() {
        return Ok(Value::from(""));
    }
    let invalid = || {
        Argument("gmp_export", 2, "word_size").value_error("is too large for argument #1 ($num)")
    };
    if layout.size > i64::MAX as usize / 4 {
        return Err(invalid());
    }
    let digits = number.magnitude().to_bytes_be();
    let length = digits
        .len()
        .div_ceil(layout.size)
        .checked_mul(layout.size)
        .ok_or_else(invalid)?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(length).map_err(|_| invalid())?;
    bytes.resize(length - digits.len(), 0);
    bytes.extend(digits);
    layout.permute(&mut bytes);
    Ok(Value::from(bytes))
}
