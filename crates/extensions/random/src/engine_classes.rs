//! The engine interfaces and classes. An engine object owns its state, and
//! `generate()` returns its next draw as little-endian bytes:
//!
//! ```php
//! $engine = new Random\Engine\Xoshiro256StarStar(42);
//! strlen($engine->generate());   // 8
//! $engine->jump();               // 2^128 draws ahead
//! (new Random\Engine\Secure())->generate();
//! ```

use std::rc::Rc;

use phera_extension_api::{
    ClassEntry, ClassHooks, ClassKind, Context, MethodEntry, NativeClone, NativeError,
    NativeResult, Object, Param, ParamType, Value, args,
};

use crate::engines::{self, Engine, XOSHIRO_JUMP, XOSHIRO_LONG_JUMP, fill, secure_u64};
use crate::mt19937::{Mt19937, reject_legacy_mode};

/// In registration order: interfaces before the classes that implement them.
pub const CLASSES: &[ClassEntry] = &[
    ENGINE,
    CRYPTO_SAFE_ENGINE,
    MT19937,
    PCG_ONESEQ_128_XSL_RR_64,
    XOSHIRO_256_STAR_STAR,
    SECURE,
];

const ENGINE: ClassEntry = ClassEntry {
    methods: &[MethodEntry::required("generate", &[])],
    ..ClassEntry::interface("Random\\Engine")
};

/// An engine fit for keys and tokens; only `Secure` is.
const CRYPTO_SAFE_ENGINE: ClassEntry = ClassEntry {
    interfaces: &["Random\\Engine"],
    ..ClassEntry::interface("Random\\CryptoSafeEngine")
};

/// `generate(): string`, shared by every engine.
const GENERATE: MethodEntry = MethodEntry::public("generate", &[], generate);

fn generate(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_engine(this, |engine| {
        let (value, size) = engine.generate()?;
        Ok(Value::from(&value.to_le_bytes()[..size]))
    })
}

/// Runs `run` on the engine inside `this`. A `Secure` engine has no
/// constructor, so its state is made on first use.
pub fn with_engine(
    this: Option<&Rc<Object>>,
    run: impl FnOnce(&mut Engine) -> NativeResult,
) -> NativeResult {
    let object = this.expect("engine methods are instance methods");
    let mut native = object.native.borrow_mut();
    if native.is_none() && object.class.name() == "Random\\Engine\\Secure" {
        *native = Some(Box::new(Engine::Secure));
    }
    let engine = native
        .as_mut()
        .and_then(|state| state.downcast_mut::<Engine>())
        .ok_or_else(|| NativeError::error("Random engine is not initialized"))?;
    run(engine)
}

fn store(this: Option<&Rc<Object>>, engine: Engine) -> NativeResult {
    let object = this.expect("constructors are instance methods");
    *object.native.borrow_mut() = Some(Box::new(engine));
    Ok(Value::Null)
}

const ENGINE_HOOKS: ClassHooks = ClassHooks {
    clone: Some(NativeClone::copy::<Engine>()),
    property_access: Some(&ENGINE_PROPERTIES),
    ..ClassHooks::EMPTY
};

/// `final class Random\Engine\Mt19937`
const MT19937: ClassEntry = ClassEntry {
    hooks: Some(&ENGINE_HOOKS),
    kind: ClassKind::FinalClass,
    interfaces: &["Random\\Engine"],
    methods: &[
        MethodEntry::public(
            "__construct",
            &[
                Param::new("seed", ParamType::Int).nullable().optional(),
                Param::new("mode", ParamType::Int).optional(),
            ],
            mt19937_construct,
        ),
        GENERATE,
        crate::serialization::SERIALIZE,
        crate::serialization::UNSERIALIZE,
    ],
    ..ClassEntry::class("Random\\Engine\\Mt19937")
};

/// `__construct(?int $seed = null, int $mode = MT_RAND_MT19937)`; a null
/// seed is random. Only the low 32 bits of the seed count.
fn mt19937_construct(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let mode = args::int(arguments, 1).unwrap_or_default();
    reject_legacy_mode(mode)?;
    if mode != crate::mt19937::MT_RAND_MT19937 {
        return Err(NativeError::value_error(
            "Random\\Engine\\Mt19937::__construct(): Argument #2 ($mode) must be either \
             MT_RAND_MT19937 or MT_RAND_PHP",
        ));
    }
    let seed = match args::int(arguments, 0) {
        Some(seed) => seed as u32,
        None => secure_u64()? as u32,
    };
    store(this, Engine::Mt(Box::new(Mt19937::new(seed))))
}

/// `final class Random\Engine\PcgOneseq128XslRr64`
const PCG_ONESEQ_128_XSL_RR_64: ClassEntry = ClassEntry {
    hooks: Some(&ENGINE_HOOKS),
    kind: ClassKind::FinalClass,
    interfaces: &["Random\\Engine"],
    methods: &[
        MethodEntry::public(
            "__construct",
            &[Param::new("seed", ParamType::IntOrString)
                .nullable()
                .optional()],
            pcg_construct,
        ),
        GENERATE,
        crate::serialization::SERIALIZE,
        crate::serialization::UNSERIALIZE,
        MethodEntry::public("jump", &[Param::new("advance", ParamType::Int)], pcg_jump),
    ],
    ..ClassEntry::class("Random\\Engine\\PcgOneseq128XslRr64")
};

/// `__construct(string|int|null $seed = null)`: 16 bytes, an int, or
/// random when null.
fn pcg_construct(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let seed = match arguments.first() {
        Some(Value::Int(seed)) => u128::from(*seed as u64),
        Some(Value::String(seed)) => {
            let Ok(bytes) = <[u8; 16]>::try_from(seed.as_bytes()) else {
                return Err(NativeError::value_error(
                    "Random\\Engine\\PcgOneseq128XslRr64::__construct(): Argument #1 ($seed) \
                     must be a 16 byte (128 bit) string",
                ));
            };
            // Two little-endian halves, the first one high.
            let (high, low) = bytes.split_at(8);
            let high = u64::from_le_bytes(high.try_into().expect("8 of 16 bytes"));
            let low = u64::from_le_bytes(low.try_into().expect("8 of 16 bytes"));
            (u128::from(high) << 64) | u128::from(low)
        }
        _ => (u128::from(secure_u64()?) << 64) | u128::from(secure_u64()?),
    };
    store(this, Engine::pcg(seed))
}

/// `jump(int $advance): void`
fn pcg_jump(_: &mut dyn Context, this: Option<&Rc<Object>>, arguments: &[Value]) -> NativeResult {
    let advance = args::int(arguments, 0).unwrap_or_default();
    if advance < 0 {
        return Err(NativeError::value_error(
            "Random\\Engine\\PcgOneseq128XslRr64::jump(): Argument #1 ($advance) must be \
             greater than or equal to 0",
        ));
    }
    with_engine(this, |engine| {
        if let Engine::Pcg(state) = engine {
            engines::pcg_jump(state, advance as u64);
        }
        Ok(Value::Null)
    })
}

/// `final class Random\Engine\Xoshiro256StarStar`
const XOSHIRO_256_STAR_STAR: ClassEntry = ClassEntry {
    hooks: Some(&ENGINE_HOOKS),
    kind: ClassKind::FinalClass,
    interfaces: &["Random\\Engine"],
    methods: &[
        MethodEntry::public(
            "__construct",
            &[Param::new("seed", ParamType::IntOrString)
                .nullable()
                .optional()],
            xoshiro_construct,
        ),
        GENERATE,
        crate::serialization::SERIALIZE,
        crate::serialization::UNSERIALIZE,
        MethodEntry::public("jump", &[], xoshiro_jump),
        MethodEntry::public("jumpLong", &[], xoshiro_jump_long),
    ],
    ..ClassEntry::class("Random\\Engine\\Xoshiro256StarStar")
};

/// `__construct(string|int|null $seed = null)`: 32 bytes that are not all
/// zero, an int, or random when null.
fn xoshiro_construct(
    _: &mut dyn Context,
    this: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    if let Some(Value::Int(seed)) = arguments.first() {
        return store(this, Engine::xoshiro(*seed as u64));
    }
    let mut random = [0; 32];
    let bytes = match arguments.first() {
        Some(Value::String(seed)) => seed.as_bytes(),
        _ => {
            fill(&mut random)?;
            &random
        }
    };
    if bytes.len() != 32 {
        return Err(NativeError::value_error(
            "Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must be a \
             32 byte (256 bit) string",
        ));
    }
    // An all-zero state would only ever produce zeros.
    if bytes.iter().all(|&byte| byte == 0) {
        return Err(NativeError::value_error(
            "Random\\Engine\\Xoshiro256StarStar::__construct(): Argument #1 ($seed) must not \
             consist entirely of NUL bytes",
        ));
    }
    let mut state = [0; 4];
    for (word, bytes) in state.iter_mut().zip(bytes.as_chunks::<8>().0) {
        *word = u64::from_le_bytes(*bytes);
    }
    store(this, Engine::Xoshiro(state))
}

/// `jump(): void`, 2^128 draws ahead.
fn xoshiro_jump(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_xoshiro(this, XOSHIRO_JUMP)
}

/// `jumpLong(): void`, 2^192 draws ahead.
fn xoshiro_jump_long(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    with_xoshiro(this, XOSHIRO_LONG_JUMP)
}

fn with_xoshiro(this: Option<&Rc<Object>>, polynomial: [u64; 4]) -> NativeResult {
    with_engine(this, |engine| {
        if let Engine::Xoshiro(state) = engine {
            engines::xoshiro_jump(state, polynomial);
        }
        Ok(Value::Null)
    })
}

/// `final class Random\Engine\Secure`: the operating system's generator.
const SECURE: ClassEntry = ClassEntry {
    hooks: Some(&ClassHooks {
        clone: Some(NativeClone::Forbidden),
        ..ClassHooks::EMPTY
    }),
    kind: ClassKind::FinalClass,
    interfaces: &["Random\\CryptoSafeEngine"],
    methods: &[GENERATE],
    ..ClassEntry::class("Random\\Engine\\Secure")
};

/// PHP's seeded native engines disallow dynamic properties, including clone-with updates.
const ENGINE_PROPERTIES: phera_extension_api::NativePropertyAccess =
    phera_extension_api::NativePropertyAccess {
        string_property: None,
        read: |_, _, _, _| Ok(None),
        mutate: |_, object, name, operation| match operation {
            phera_extension_api::NativePropertyMutation::Assign(_) => {
                Err(NativeError::error(format!(
                    "Cannot create dynamic property {}::${name}",
                    object.class.name()
                )))
            }
            _ => Ok(false),
        },
    };
