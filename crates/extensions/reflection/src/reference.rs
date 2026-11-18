//! `ReflectionReference::fromArrayElement($array, $key)` compares reference identity.

use std::cell::RefCell;
use std::collections::{HashMap, hash_map::RandomState};
use std::hash::{BuildHasher, Hasher};
use std::rc::{Rc, Weak};

use phera_extension_api::{
    ArrayKey, ClassEntry, ClassKind, Context, MethodEntry, NativeResult, Object, Param, ParamType,
    Registry, Value, Visibility, args,
};

use crate::{UNCLONEABLE, allocate, exception, load};

#[derive(Default)]
struct ReferenceIds {
    salt: RandomState,
    next: u64,
    entries: HashMap<*const RefCell<Value>, ReferenceIdentity>,
}

struct ReferenceIdentity {
    // A weak owner keeps this address distinct even after the reference dies.
    identity: Weak<RefCell<Value>>,
    id: [u8; 20],
}

pub(crate) fn register(registry: &mut Registry) {
    registry.class(ClassEntry {
        kind: ClassKind::FinalClass,
        hooks: Some(&UNCLONEABLE),
        methods: const {
            &[
                MethodEntry::public("__construct", &[], |_, _, _| Ok(Value::Null))
                    .with_visibility(Visibility::Private),
                MethodEntry::public(
                    "fromArrayElement",
                    const {
                        &[
                            Param::new("array", ParamType::Array),
                            Param::new("key", ParamType::IntOrString),
                        ]
                    },
                    from_array,
                )
                .static_()
                .returning("?ReflectionReference"),
                MethodEntry::public("getId", &[], get_id).returning("string"),
            ]
        },
        ..ClassEntry::class("ReflectionReference")
    });
}

fn from_array(
    context: &mut dyn Context,
    _: Option<&Rc<Object>>,
    arguments: &[Value],
) -> NativeResult {
    let array = args::array(arguments, 0).ok_or_else(|| exception("Expected an array"))?;
    let key = match arguments.get(1) {
        Some(Value::Int(key)) => ArrayKey::Int(*key),
        Some(Value::String(key)) => ArrayKey::String(key.clone()),
        _ => return Err(exception("Array key not found")),
    };
    let value = array
        .get(&key)
        .ok_or_else(|| exception("Array key not found"))?;
    let Value::Reference(reference) = value else {
        return Ok(Value::Null);
    };
    if Rc::strong_count(reference) == 1 {
        let recursive =
            matches!(&*reference.borrow(), Value::Array(nested) if Rc::ptr_eq(array, nested));
        if !recursive {
            return Ok(Value::Null);
        }
    }
    let ids = context.state().get_or_default::<ReferenceIds>();
    let address = Rc::as_ptr(reference);
    let id = if let Some(existing) = ids.entries.get(&address) {
        existing.id
    } else {
        if ids.entries.len().is_power_of_two() {
            ids.entries
                .retain(|_, entry| entry.identity.strong_count() != 0);
        }
        let number = ids.next;
        ids.next = ids.next.wrapping_add(1);
        let mut id = [0u8; 20];
        id[..8].copy_from_slice(&number.to_ne_bytes());
        let mut salt = ids.salt.build_hasher();
        salt.write_u64(number);
        id[8..16].copy_from_slice(&salt.finish().to_ne_bytes());
        let mut check = ids.salt.build_hasher();
        check.write_u64(number);
        check.write_u8(1);
        id[16..].copy_from_slice(&(check.finish() as u32).to_ne_bytes());
        ids.entries.insert(
            address,
            ReferenceIdentity {
                identity: Rc::downgrade(reference),
                id,
            },
        );
        id
    };
    allocate(
        context,
        "ReflectionReference",
        None,
        (id, reference.clone()),
    )
}

fn get_id(_: &mut dyn Context, this: Option<&Rc<Object>>, _: &[Value]) -> NativeResult {
    let id = load::<[u8; 20]>(this)?;
    Ok(Value::from(id.to_vec()))
}
