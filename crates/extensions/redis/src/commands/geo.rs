//! Geo coordinates travel as decimal arguments, while only Redis keys receive prefixes.
use super::*;

pub fn build(name: &str, a: &[Value], b: &mut Builder<'_>) -> Result<bool, NativeError> {
    match name {
        "geoadd" => {
            b.key(&a[0]);
            b.args(&a[1..]);
        }
        "geodist" => {
            b.key(&a[0]);
            b.args(&a[1..]);
            b.shape = Shape::Float;
        }
        "geohash" | "geopos" => {
            b.key(&a[0]);
            b.args(&a[1..]);
            if name == "geopos" {
                b.shape = Shape::Coordinates;
            }
        }
        "georadius" | "georadius_ro" | "georadiusbymember" | "georadiusbymember_ro" => {
            b.key(&a[0]);
            let end = if name.contains("bymember") { 4 } else { 5 };
            b.args(&a[1..end]);
            flags(b, a.get(end));
        }
        "geosearch" | "geosearchstore" => {
            let offset = usize::from(name == "geosearchstore");
            b.key(&a[0]);
            if offset > 0 {
                b.key(&a[1]);
            }
            if let Some(position) = array(a.get(offset + 1)) {
                b.token("FROMLONLAT");
                for (_, value) in position.iter() {
                    b.arg(value);
                }
            } else {
                b.token("FROMMEMBER");
                b.arg(&a[offset + 1]);
            }
            if let Some(shape) = array(a.get(offset + 2)) {
                b.token("BYBOX");
                for (_, value) in shape.iter() {
                    b.arg(value);
                }
            } else {
                b.token("BYRADIUS");
                b.arg(&a[offset + 2]);
            }
            b.arg(&a[offset + 3]);
            flags(b, a.get(offset + 4));
        }
        _ => return Ok(false),
    }
    Ok(true)
}
