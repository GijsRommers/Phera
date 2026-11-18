//! Monomorphic property sites retain visibility decisions, never property values.

use crate::{
    AccessPath, Vm,
    property_targets::{PropertyShape, PropertyTarget},
};
use phera_bytecode::{NameId, Step, TypeMask};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, ClassHandle, Object, Value};
use std::rc::Rc;

/// Entries are made only after successful property_key resolution. A different
/// receiver class or lexical scope replaces the entry. UnitCaches retains units,
/// entries retain classes, and the entire map dies with the request, preventing
/// address reuse and conditional declarations in another request from matching.
/// Class and lexical scope identities guard the slow visibility resolver's result.
#[derive(Clone)]
pub(crate) struct PropertyEntry {
    class: Rc<dyn ClassHandle>,
    scope: Option<Rc<crate::classes::Class>>,
    key: ArrayKey,
    write_rule: WriteRule,
    pub(crate) slot: Option<usize>,
    /// An immutable class fact, so a property-site hit avoids a class downcast.
    read_without_static_notice: bool,
}

impl Vm {
    /// Share immutable entries so cache hits avoid copying class, scope and property-key owners.
    pub(crate) fn property_entry(
        &mut self,
        object: &Object,
        name: NameId,
    ) -> Result<Option<Rc<PropertyEntry>>, NativeError> {
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("property site"))?;
        let scope = frame.scope();
        if let Some(slot) = frame.site_cache()
            && let crate::unit_caches::CachedSite::Property(entry) = &*slot.borrow()
            && Rc::ptr_eq(&entry.class, &object.class)
            && match (&entry.scope, &scope) {
                (None, None) => true,
                (Some(left), Some(right)) => Rc::ptr_eq(left, right),
                _ => false,
            }
        {
            return Ok(Some(entry.clone()));
        }
        if crate::properties::native_property_access(object).is_some() {
            return Ok(None);
        }
        let scope = scope.cloned();
        let unit = frame.unit().clone();
        let name = unit
            .names
            .get(name.index())
            .ok_or_else(|| internal_error!("property name"))?;
        let class = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>();
        let key = match self.property_key(object, name) {
            Ok(key) => key,
            Err(_) if has_magic_properties(object) => return Ok(None),
            Err(error) => return Err(error),
        };
        let property = class.and_then(|class| {
            class
                .defaults
                .iter()
                .enumerate()
                .find(|(_, property)| property.key == key)
        });
        let slot = property.map(|(slot, _)| slot);
        let dispatched = property.and_then(|(_, property)| {
            let hooks = property.hooks.as_ref()?;
            (hooks.hooks.is_virtual
                || !this_property_site(frame)
                || !self.uses_property_backing(object, property, hooks))
            .then_some(hooks)
        });
        let invokes_getter =
            dispatched.is_some_and(|hooks| hooks.hooks.is_virtual || hooks.hooks.get.is_some());
        let write_rule = match property {
            Some((_, property)) if !property.is_readonly && dispatched.is_none() => {
                match &property.ty {
                    None => WriteRule::Untyped,
                    Some(declared) => WriteRule::Typed(declared.mask),
                }
            }
            _ => WriteRule::General,
        };
        let entry = Rc::new(PropertyEntry {
            class: object.class.clone(),
            scope,
            key,
            write_rule,
            slot,
            read_without_static_notice: !invokes_getter
                && class.is_none_or(|class| {
                    class
                        .static_properties
                        .iter()
                        .all(|property| property.name != *name)
                }),
        });
        if let Some(slot) = self.current().and_then(|frame| frame.site_cache()) {
            *slot.borrow_mut() = crate::unit_caches::CachedSite::Property(entry.clone());
        }
        Ok(Some(entry))
    }

    /// Mirrors read_place/read_property for an existing single property. Missing
    /// slots fall through for typed initialization errors and undefined warnings.
    pub(crate) fn read_property_site(
        &mut self,
        place: &AccessPath,
    ) -> Result<Option<Value>, NativeError> {
        let Some(PropertyTarget { object, name }) = self.read_property_target(place)? else {
            return Ok(None);
        };
        let Some(entry) = self.property_entry(&object, name)? else {
            return Ok(None);
        };
        if !entry.read_without_static_notice {
            return Ok(None);
        }
        Ok(
            crate::property_storage::read_at(&object, &entry.key, entry.slot).map(|value| {
                match value {
                    Value::Reference(_) => value.deref().clone(),
                    value => value,
                }
            }),
        )
    }

    /// Mirrors inc_dec_path and validated write_place only for an existing,
    /// nonreference integer in a mutable int/untyped declaration. Every other
    /// case retains coercion, readonly checks and reference tracking in the slow path.
    pub(crate) fn increment_property_site(
        &mut self,
        place: &AccessPath,
        mode: phera_bytecode::IncDec,
    ) -> Result<Option<Value>, NativeError> {
        use phera_bytecode::IncDec;
        let Some(PropertyTarget { object, name }) =
            self.property_target(place, PropertyShape::Single)?
        else {
            return Ok(None);
        };
        let Some(entry) = self.property_entry(&object, name)? else {
            return Ok(None);
        };
        if !entry.write_rule.allows(TypeMask::INT) {
            return Ok(None);
        }
        let Some(Value::Int(old)) =
            crate::property_storage::read_at(&object, &entry.key, entry.slot)
        else {
            return Ok(None);
        };
        let delta = if matches!(mode, IncDec::PreIncrement | IncDec::PostIncrement) {
            1
        } else {
            -1
        };
        let Some(new) = old.checked_add(delta) else {
            return Ok(None);
        };
        crate::property_storage::write_at(&object, entry.key.clone(), entry.slot, Value::Int(new));
        Ok(Some(Value::Int(
            if matches!(mode, IncDec::PostIncrement | IncDec::PostDecrement) {
                old
            } else {
                new
            },
        )))
    }
}

/// What a cached write may store without running the full type check.
#[derive(Clone, Copy)]
enum WriteRule {
    /// The property has no declared type: any value fits.
    Untyped,
    /// The property has a declared type: a value fits when its own built-in
    /// type is in this set (`int` for `public int $n`, `int` or `null` for
    /// `?int`). A class type has no bit here, so objects take the full check.
    Typed(TypeMask),
    /// Readonly, or not a declared property: always take the full path.
    General,
}

impl WriteRule {
    /// Whether a value of this built-in type may be stored as it is.
    fn allows(self, own_type: TypeMask) -> bool {
        match self {
            WriteRule::Untyped => true,
            WriteRule::Typed(declared) => declared.intersects(own_type),
            WriteRule::General => false,
        }
    }
}

impl Vm {
    /// Mirrors validated write_place for mutable declarations and already matching
    /// values. Existing references retain write-through and type guards in the slow path.
    pub(crate) fn write_property_site(
        &mut self,
        place: &AccessPath,
        value: &Value,
    ) -> Result<bool, NativeError> {
        let Some(PropertyTarget { object, name }) =
            self.property_target(place, PropertyShape::Single)?
        else {
            return Ok(false);
        };
        let Some(entry) = self.property_entry(&object, name)? else {
            return Ok(false);
        };
        // `public float $x` assigned an int stores the float, as PHP does.
        let widened = match entry.write_rule {
            WriteRule::Typed(declared) => crate::types::widen_int_to_float(value, declared),
            _ => None,
        };
        let value = widened.as_ref().unwrap_or(value);
        let accepted = !matches!(value, Value::Reference(_))
            && entry.write_rule.allows(crate::types::own_type(value));
        if !accepted {
            return Ok(false);
        }
        let existing = crate::property_storage::read_at(&object, &entry.key, entry.slot);
        if matches!(existing, Some(Value::Reference(_)))
            || (existing.is_none() && has_magic_properties(&object))
        {
            return Ok(false);
        }
        crate::property_storage::write_at(&object, entry.key.clone(), entry.slot, value.clone());
        Ok(true)
    }

    /// Mirrors property-key validation and path reconstruction for an existing
    /// mutable array property. Reference cells and readonly declarations fall back.
    pub(crate) fn append_property_site(
        &mut self,
        place: &AccessPath,
        value: &Value,
    ) -> Result<bool, NativeError> {
        let Some(PropertyTarget { object, name }) =
            self.property_target(place, PropertyShape::Append)?
        else {
            return Ok(false);
        };
        let Some(entry) = self.property_entry(&object, name)? else {
            return Ok(false);
        };
        if !entry.write_rule.allows(TypeMask::ARRAY) {
            return Ok(false);
        }
        match crate::property_storage::append_at(&object, &entry.key, entry.slot, value) {
            Some(result) => {
                result?;
                Ok(true)
            }
            None => Ok(false),
        }
    }
}

impl Vm {
    /// Array-only paths take one predictable rejection branch before property work.
    pub(crate) fn assign_property_site(
        &mut self,
        place: &AccessPath,
        value: &Value,
    ) -> Result<bool, NativeError> {
        if !matches!(place.steps()?.next(), Some(Step::Property(_))) {
            return Ok(false);
        }
        Ok(self.write_property_site(place, value)? || self.append_property_site(place, value)?)
    }
}

fn has_magic_properties(object: &Object) -> bool {
    object
        .class
        .as_any()
        .downcast_ref::<crate::classes::Class>()
        .and_then(|class| class.magic.as_ref())
        .is_some_and(|hooks| hooks.has_properties())
}

/// Cache filling can prove that a literal receiver is the current hook's own
/// $this. Arbitrary register or indexed receivers still check object identity.
fn this_property_site(frame: crate::calls::frame::FrameView<'_>) -> bool {
    use phera_bytecode::{Instruction, PathBase};
    let function = frame.function();
    let path = match function.instructions.get(frame.ip().saturating_sub(1)) {
        Some(
            Instruction::Fetch { path, .. }
            | Instruction::FetchIndex { path, .. }
            | Instruction::AssignPath { path, .. }
            | Instruction::AssignPathFromLocal { path, .. }
            | Instruction::IncDecPath { path, .. }
            | Instruction::SendPath { path },
        ) => path,
        _ => return false,
    };
    function
        .paths
        .get(path.index())
        .is_some_and(|path| matches!(path.base, PathBase::This { .. }))
}
