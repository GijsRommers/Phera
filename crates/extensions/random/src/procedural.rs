//! `mt_srand(42); rand(1,6)` and shuffles consume the same per-request generator.
use crate::{
    engines::{self, Engine},
    mt19937::{Mode, Mt19937},
};
use phera_extension_api::{Context, LocalFuture, NativeError, NativeResult, Value, args};
#[derive(Default)]
struct RequestRandom {
    engine: Option<Engine>,
    lcg: Option<[i64; 2]>,
    last_id: u128,
}
pub fn generator(context: &mut dyn Context) -> Result<&mut Engine, NativeError> {
    let state = context.state().get_or_default::<RequestRandom>();
    if state.engine.is_none() {
        state.engine = Some(Engine::Mt(Box::new(Mt19937::new(
            engines::secure_u64()? as u32,
            Mode::Standard,
        ))));
    }
    Ok(state.engine.as_mut().expect("initialized above"))
}
pub fn seed(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let mode = if args::int(arguments, 1) == Some(1) {
        context.deprecated("The MT_RAND_PHP variant of Mt19937 is deprecated".into());
        Mode::Legacy
    } else {
        Mode::Standard
    };
    let seed = match args::int(arguments, 0) {
        Some(seed) => seed as u32,
        None => engines::secure_u64()? as u32,
    };
    context.state().get_or_default::<RequestRandom>().engine =
        Some(Engine::Mt(Box::new(Mt19937::new(seed, mode))));
    Ok(Value::Null)
}
pub fn mt_rand(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    draw(context, arguments, "mt_rand", false)
}
pub fn rand(context: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    draw(context, arguments, "rand", true)
}
fn draw(context: &mut dyn Context, arguments: &[Value], name: &str, swap: bool) -> NativeResult {
    if arguments.is_empty() {
        return Ok(Value::Int((generator(context)?.generate()?.0 >> 1) as i64));
    }
    // PHP permits either zero or exactly two arguments. Its count check precedes coercion.
    const RANGE: &[phera_extension_api::Param] = &[
        phera_extension_api::Param::new("min", phera_extension_api::ParamType::Int),
        phera_extension_api::Param::new("max", phera_extension_api::ParamType::Int),
    ];
    let mut prepared = arguments.to_vec();
    phera_extension_api::prepare_call(name, RANGE, &mut prepared, context)?;
    let arguments = &prepared;
    let mut min = args::int(arguments, 0).unwrap_or_default();
    let mut max = args::int(arguments, 1).unwrap_or_default();
    if min > max {
        if swap {
            std::mem::swap(&mut min, &mut max);
        } else {
            return Err(NativeError::value_error(
                "mt_rand(): Argument #2 ($max) must be greater than or equal to argument #1 ($min)",
            ));
        }
    }
    Ok(Value::Int(generator(context)?.integer(min, max)?))
}
pub fn max(_: &mut dyn Context, _: &[Value]) -> NativeResult {
    Ok(Value::Int(2147483647))
}
pub fn random_int(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let min = args::int(arguments, 0).unwrap_or_default();
    let max = args::int(arguments, 1).unwrap_or_default();
    if min > max {
        return Err(NativeError::value_error(
            "random_int(): Argument #1 ($min) must be less than or equal to argument #2 ($max)",
        ));
    }
    if min == max {
        return Ok(Value::Int(min));
    }
    Ok(Value::Int(Engine::Secure.range(min, max)?))
}
pub fn buffer(length: usize) -> Result<Vec<u8>, NativeError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| NativeError::error("Insufficient memory"))?;
    bytes.resize(length, 0);
    Ok(bytes)
}
pub fn random_bytes(_: &mut dyn Context, arguments: &[Value]) -> NativeResult {
    let length = args::int(arguments, 0).unwrap_or_default();
    if length < 1 {
        return Err(NativeError::value_error(
            "random_bytes(): Argument #1 ($length) must be greater than 0",
        ));
    }
    let mut bytes = buffer(length as usize)?;
    engines::fill(&mut bytes)?;
    Ok(Value::from(bytes))
}
pub fn lcg(context: &mut dyn Context, _: &[Value]) -> NativeResult {
    context.deprecated(
        "Function lcg_value() is deprecated since 8.4, use \
                \\Random\\Randomizer::getFloat() instead"
            .into(),
    );
    let state = context.state().get_or_default::<RequestRandom>();
    if state.lcg.is_none() {
        let seed = engines::secure_u64()?;
        state.lcg = Some([i64::from(seed as i32), i64::from((seed >> 32) as i32)]);
    }
    let values = state.lcg.as_mut().expect("initialized above");
    for (s, (a, b, c, m)) in values.iter_mut().zip([
        (53668, 40014, 12211, 2147483563),
        (52774, 40692, 3791, 2147483399),
    ]) {
        let quotient = *s / a;
        *s = b * (*s - a * quotient) - c * quotient;
        if *s < 0 {
            *s += m;
        }
    }
    let mut z = values[0] - values[1];
    if z < 1 {
        z += 2147483562;
    }
    Ok(Value::Float(z as f64 * 4.656613e-10))
}
pub fn uniqid(context: &mut dyn Context, arguments: Vec<Value>) -> LocalFuture<'_, NativeResult> {
    Box::pin(async move {
        let now = loop {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(|e| NativeError::error(e.to_string()))?;
            let state = context.state().get_or_default::<RequestRandom>();
            if state.last_id != now.as_micros() {
                state.last_id = now.as_micros();
                break now;
            }
            tokio::task::yield_now().await;
        };
        let mut result = args::bytes(&arguments, 0)
            .split(|&b| b == 0)
            .next()
            .unwrap_or_default()
            .to_vec();
        result.extend_from_slice(
            format!("{:08x}{:05x}", now.as_secs() as u32, now.subsec_micros()).as_bytes(),
        );
        if args::bool(&arguments, 1).unwrap_or(false) {
            let seed = engines::secure_u64()? as u32;
            result.extend_from_slice(
                format!("{:.8}", f64::from(seed) / f64::from(u32::MAX) * 10.0).as_bytes(),
            );
        }
        Ok(Value::from(result))
    })
}
