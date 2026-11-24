//! Monomorphic property sites retain visibility decisions, never property values.

use crate::classes::property_facts::PropertyFacts;
use crate::{
    AccessPath, Vm,
    property_hook_sites::CachedPropertyHooks,
    property_targets::{PropertyShape, PropertyTarget},
};
use phera_bytecode::{NameId, Reg, Step, TypeMask};
use phera_extension_api::NativeError;
use phera_value::{ArrayKey, ClassHandle, Object, Value};
use std::rc::Rc;

/// Entries are made only after successful property_key resolution. A different
/// receiver class or lexical scope replaces the entry. UnitCaches retains units,
/// entries retain classes, and the entire map dies with the request, preventing
/// address reuse and conditional declarations in another request from matching.
/// Class and lexical scope identities guard the slow visibility resolver's result.
pub(crate) struct PropertyEntry {
    class: Rc<dyn ClassHandle>,
    scope: Option<Rc<crate::classes::Class>>,
    key: ArrayKey,
    write_rule: WriteRule,
    /// A compact index leaves room for the optional hook record without
    /// increasing the size of the cached entry used for ordinary properties.
    slot: Option<u32>,
    /// An immutable class fact, so a property-site hit avoids a class downcast.
    read_without_static_notice: bool,
    pub(crate) hooks: Option<Box<CachedPropertyHooks>>,
    /// Direct PHP `__get($name)` handler for undeclared property reads on
    /// classes without native properties or same-named statics.
    pub(crate) magic_get: Option<crate::code_table::FunctionIndex>,
    /// Direct PHP `__set($name, $value)` handler for undeclared property writes
    /// on classes without native properties or same-named statics.
    pub(crate) magic_set: Option<crate::code_table::FunctionIndex>,
}

impl PropertyEntry {
    fn slot(&self) -> Option<usize> {
        self.slot.map(|slot| slot as usize)
    }

    /// The property as this entry knows it, for an object it has matched.
    fn known(&self) -> crate::property_storage::Known<'_> {
        crate::property_storage::Known::of(&self.key, self.slot())
    }

    /// The integer increment of [`Vm::increment_property_site`], once the
    /// entry is known to fit the object.
    fn increment(&self, object: &Object, mode: phera_bytecode::IncDec) -> Option<Value> {
        if !self.write_rule.allows(TypeMask::INT) {
            return None;
        }
        crate::property_storage::increment_known(object, self.known(), mode)
    }

    fn read(&self, object: &Object) -> Option<Value> {
        if !self.read_without_static_notice {
            return None;
        }
        crate::property_storage::read_known(object, self.known()).map(|value| match value {
            Value::Reference(_) => value.deref().clone(),
            value => value,
        })
    }
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
        if let Some(entry) = self
            .code
            .read_site(frame.frame.function, frame.ip(), |site| {
                let crate::unit_caches::CachedSite::Property(entry) = site else {
                    return None;
                };
                if Rc::ptr_eq(&entry.class, &object.class)
                    && match (&entry.scope, &scope) {
                        (None, None) => true,
                        (Some(left), Some(right)) => Rc::ptr_eq(left, right),
                        _ => false,
                    }
                {
                    Some(entry.clone())
                } else {
                    None
                }
            })
            .flatten()
        {
            return Ok(Some(entry));
        }
        self.fill_property_entry(object, name)
    }

    #[cold]
    #[inline(never)]
    fn fill_property_entry(
        &mut self,
        object: &Object,
        name: NameId,
    ) -> Result<Option<Rc<PropertyEntry>>, NativeError> {
        if crate::properties::native_property_access(object).is_some() {
            return Ok(None);
        }
        let frame = self
            .current()
            .ok_or_else(|| internal_error!("property site"))?;
        let scope = frame.scope().cloned();
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
        let slot = property.and_then(|(slot, _)| u32::try_from(slot).ok());
        let dispatched =
            property.and_then(|(_, property)| {
                let hooks = property.hooks.as_ref()?;
                let guard_receiver = self.has_property_hook_scope(property, hooks);
                (hooks.hooks.is_virtual || !guard_receiver || !this_property_site(frame))
                    .then_some((property, hooks, guard_receiver))
            });
        let invokes_getter = dispatched
            .is_some_and(|(_, hooks, _)| hooks.hooks.is_virtual || hooks.hooks.get.is_some());
        let write_rule = match property {
            Some((_, property))
                if !property.is_readonly
                    && property.set_visibility.is_none()
                    && dispatched.is_none() =>
            {
                match &property.ty {
                    None => WriteRule::Untyped,
                    Some(declared) => match self.property_type_classes(declared, property) {
                        Some(classes) => WriteRule::TypedClasses(declared.mask, classes),
                        None => WriteRule::Typed(declared.mask),
                    },
                }
            }
            // A name a static property of the class has gets PHP's notice
            // on the general path.
            None if class.is_some_and(|class| {
                class
                    .static_properties
                    .iter()
                    .any(|property| property.name == *name)
            }) =>
            {
                WriteRule::General
            }
            None => match class.map(crate::classes::Class::property_facts) {
                Some(facts)
                    if !facts.has(PropertyFacts::MAGIC_SET)
                        && !facts.has(PropertyFacts::HOOKS)
                        && !facts.has(PropertyFacts::READONLY) =>
                {
                    WriteRule::Undeclared(facts.has(PropertyFacts::DYNAMIC))
                }
                _ => WriteRule::General,
            },
            _ => WriteRule::General,
        };
        let read_without_static_notice = !invokes_getter
            && class.is_none_or(|class| {
                class
                    .static_properties
                    .iter()
                    .all(|property| property.name != *name)
            });
        let plain_magic_eligible = slot.is_none()
            && read_without_static_notice
            && class.is_some_and(|class| {
                class.property_access.is_none()
                    && !phera_value::ClassHandle::has_native_properties(class)
            });
        let magic_get = if plain_magic_eligible {
            class
                .and_then(|class| class.magic.as_deref())
                .and_then(|magic| magic.get.as_ref())
                .and_then(|method| {
                    if method.visibility != phera_bytecode::Visibility::Public {
                        return None;
                    }
                    let crate::classes::MethodBody::User(function) = method.body else {
                        return None;
                    };
                    if self.code.function(function).returns_reference {
                        return None;
                    }
                    let facts = self.code.facts(function);
                    if facts.shape != crate::function_facts::FunctionShape::Plain
                        || facts.parameters != 1
                        || !facts.takes_values_only()
                    {
                        return None;
                    }
                    if facts.is_typed(0)
                        && !facts
                            .parameter_masks
                            .first()
                            .is_some_and(|mask| mask.intersects(TypeMask::STRING))
                    {
                        return None;
                    }
                    Some(function)
                })
        } else {
            None
        };
        let magic_set = if plain_magic_eligible {
            class
                .and_then(|class| class.magic.as_deref())
                .and_then(|magic| magic.set.as_ref())
                .and_then(|method| {
                    if method.visibility != phera_bytecode::Visibility::Public {
                        return None;
                    }
                    let crate::classes::MethodBody::User(function) = method.body else {
                        return None;
                    };
                    let facts = self.code.facts(function);
                    if facts.shape != crate::function_facts::FunctionShape::Plain
                        || facts.parameters != 2
                        || !facts.takes_values_only()
                    {
                        return None;
                    }
                    if facts.is_typed(0)
                        && !facts
                            .parameter_masks
                            .first()
                            .is_some_and(|mask| mask.intersects(TypeMask::STRING))
                    {
                        return None;
                    }
                    Some(function)
                })
        } else {
            None
        };
        let entry = Rc::new(PropertyEntry {
            class: object.class.clone(),
            scope,
            key,
            write_rule,
            slot,
            hooks: dispatched.map(|(property, hooks, guard_receiver)| {
                Box::new(CachedPropertyHooks {
                    getter: hooks.hooks.get.as_ref().and_then(|getter| {
                        (!self.code.function(getter.function).returns_reference)
                            .then_some(getter.function)
                    }),
                    setter: hooks.hooks.set.as_ref().and_then(|setter| {
                        self.check_set_visibility(
                            property,
                            crate::property_set_visibility::PropertyChange::Direct,
                        )
                        .is_ok()
                        .then_some(setter.function)
                    }),
                    guard_receiver,
                })
            }),
            read_without_static_notice,
            magic_get,
            magic_set,
        });
        if let Some(frame) = self.current() {
            self.code.write_site(
                frame.frame.function,
                frame.ip(),
                crate::unit_caches::CachedSite::Property(entry.clone()),
            );
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
        Ok(entry.read(&object))
    }

    /// A fetch either writes its own result or supplies the getter's return destination.
    pub(crate) fn fetch_property_site(
        &mut self,
        place: &AccessPath,
        destination: Reg,
    ) -> Result<bool, NativeError> {
        let Some(PropertyTarget { object, name }) = self.read_property_target(place)? else {
            return Ok(false);
        };
        let Some(entry) = self.property_entry(&object, name)? else {
            return Ok(false);
        };
        if let Some(value) = entry.read(&object) {
            self.set(destination, value);
            return Ok(true);
        }
        if entry.hooks.is_some() {
            return self.enter_cached_getter(&entry, object, destination);
        }
        self.enter_cached_magic_get(&entry, object, destination)
    }

    /// Synchronous frame-stack entry into `__get(string $name)` for an
    /// undeclared missing property, avoiding async dispatch, `Box::pin`, and
    /// nested `run_batch`.
    #[inline(never)]
    fn enter_cached_magic_get(
        &mut self,
        entry: &PropertyEntry,
        object: Rc<Object>,
        destination: Reg,
    ) -> Result<bool, NativeError> {
        let Some(function) = entry.magic_get else {
            return Ok(false);
        };
        if object.lazy().is_some()
            || self.property_guarded(
                &object,
                &entry.key,
                crate::magic_properties::PropertyHook::Get,
            )
        {
            return Ok(false);
        }
        let name_value = match &entry.key {
            ArrayKey::Interned(name) => Value::InternedStr(name),
            ArrayKey::String(name) => Value::String(name.clone()),
            ArrayKey::Int(index) => Value::from(index.to_string()),
        };
        self.check_frame_limit()?;
        let facts = self.code.facts(function);
        let base = self.registers.len() + 1;
        self.registers
            .grow_to(base + (facts.registers as usize).max(1));
        self.registers.put(base - 1, Value::Object(object.clone()));
        self.registers.put(base, name_value);
        self.property_guards
            .push(crate::magic_properties::PropertyGuard {
                object: Rc::downgrade(&object),
                name: entry.key.clone(),
                hook: crate::magic_properties::PropertyHook::Get,
            });
        let frame = crate::calls::frame::Frame::of_kind(
            function,
            crate::calls::prepared_call::CallKind::Method,
            base,
            Some(destination),
        );
        self.call_stack.push(frame);
        self.call_stack.current_extras_mut().magic_guard = true;
        Ok(true)
    }

    /// Synchronous frame-stack entry into `__set(string $name, mixed $value)`
    /// for an undeclared property write.
    #[inline(never)]
    fn enter_cached_magic_set(
        &mut self,
        entry: &PropertyEntry,
        object: Rc<Object>,
        value: &Value,
        assignment: Option<(Reg, Reg)>,
    ) -> Result<bool, NativeError> {
        let Some(function) = entry.magic_set else {
            return Ok(false);
        };
        if matches!(value, Value::Reference(_))
            || object.lazy().is_some()
            || object.has(&entry.key)
            || self.property_guarded(
                &object,
                &entry.key,
                crate::magic_properties::PropertyHook::Set,
            )
        {
            return Ok(false);
        }
        let registers = {
            let facts = self.code.facts(function);
            if facts.is_typed(1)
                && !facts
                    .parameter_masks
                    .get(1)
                    .is_some_and(|mask| mask.intersects(crate::types::own_type(value)))
            {
                return Ok(false);
            }
            facts.registers as usize
        };
        if let Some((source, result)) = assignment {
            if self.raw_local(source).is_reference() {
                return Ok(false);
            }
            self.set(result, value.clone());
        }
        let name_value = match &entry.key {
            ArrayKey::Interned(name) => Value::InternedStr(name),
            ArrayKey::String(name) => Value::String(name.clone()),
            ArrayKey::Int(index) => Value::from(index.to_string()),
        };
        self.check_frame_limit()?;
        let base = self.registers.len() + 1;
        self.registers.grow_to(base + registers.max(2));
        self.registers.put(base - 1, Value::Object(object.clone()));
        self.registers.put(base, name_value);
        self.registers.put(base + 1, value.clone());
        self.property_guards
            .push(crate::magic_properties::PropertyGuard {
                object: Rc::downgrade(&object),
                name: entry.key.clone(),
                hook: crate::magic_properties::PropertyHook::Set,
            });
        let mut frame = crate::calls::frame::Frame::of_kind(
            function,
            crate::calls::prepared_call::CallKind::Method,
            base,
            None,
        );
        frame.discard_result();
        self.call_stack.push(frame);
        self.call_stack.current_extras_mut().magic_guard = true;
        Ok(true)
    }

    /// Mirrors inc_dec_path and validated write_place only for an existing,
    /// nonreference integer in a mutable int/untyped declaration. Every other
    /// case retains coercion, readonly checks and reference tracking in the slow path.
    pub(crate) fn increment_property_site(
        &mut self,
        place: &AccessPath,
        mode: phera_bytecode::IncDec,
    ) -> Result<Option<Value>, NativeError> {
        let Some(PropertyTarget { object, name }) =
            self.property_target(place, PropertyShape::Single)?
        else {
            return Ok(None);
        };
        let Some(entry) = self.property_entry(&object, name)? else {
            return Ok(None);
        };
        Ok(entry.increment(&object, mode))
    }
}

/// What a cached write may store without running the full type check.
#[derive(Clone)]
enum WriteRule {
    /// The property has no declared type: any value fits.
    Untyped,
    /// The property has a declared type: a value fits when its own built-in
    /// type is in this set (`int` for `public int $n`, `int` or `null` for
    /// `?int`). An object fits only an `object` type here; a type that
    /// names classes the entry could not resolve leaves objects to the
    /// full check.
    Typed(TypeMask),
    /// The same, for a type that names classes, resolved when the entry
    /// was made: an object fits when its class is one of them or extends or
    /// implements one (all of them, for an intersection).
    TypedClasses(TypeMask, Rc<ClassTargets>),
    /// A property the class does not declare, of a class with no `__set`,
    /// no hooks and nothing readonly: any value fits. One that does not
    /// exist yet is made here only when the class allows that without a
    /// deprecation (`true`: `stdClass`, `#[AllowDynamicProperties]`).
    Undeclared(bool),
    /// Readonly, or a property whose writes need more: always take the
    /// full path.
    General,
}

/// The classes a property type names, as the request has them.
pub(crate) struct ClassTargets {
    classes: Box<[Rc<crate::classes::Class>]>,
    /// `A&B`: the object's class must derive from every one.
    intersection: bool,
}

impl WriteRule {
    /// Whether a value of this built-in type may be stored as it is.
    fn allows(&self, own_type: TypeMask) -> bool {
        match self {
            WriteRule::Untyped | WriteRule::Undeclared(_) => true,
            WriteRule::Typed(declared) | WriteRule::TypedClasses(declared, _) => {
                declared.intersects(own_type)
            }
            WriteRule::General => false,
        }
    }

    /// Whether `value` may be stored as it is: its built-in type, or an
    /// object whose class the declared classes allow.
    fn accepts(&self, value: &Value) -> bool {
        if self.allows(crate::types::own_type(value)) {
            return true;
        }
        let (WriteRule::TypedClasses(_, targets), Value::Object(object)) = (self, value) else {
            return false;
        };
        let Some(class) = object
            .class
            .as_any()
            .downcast_ref::<crate::classes::Class>()
        else {
            return false;
        };
        let derives = |target: &Rc<crate::classes::Class>| class.derives_from(target) == Some(true);
        if targets.intersection {
            targets.classes.iter().all(derives)
        } else {
            targets.classes.iter().any(derives)
        }
    }

    /// Whether a write may make the property when the object does not have
    /// it yet: all but an undeclared property of a class that warns.
    fn may_create(&self) -> bool {
        !matches!(self, WriteRule::Undeclared(false))
    }

    /// `public float $x` assigned an int stores the float, as PHP does.
    fn widened(&self, value: &Value) -> Option<Value> {
        match self {
            WriteRule::Typed(declared) | WriteRule::TypedClasses(declared, _) => {
                crate::types::widen_int_to_float(value, *declared)
            }
            _ => None,
        }
    }
}

impl Vm {
    /// The classes a property's declared type names, resolved now, for a
    /// site's write rule: `self` and `parent` from the declaring class,
    /// the others by name. `None` when the type names no class, has a
    /// shape this does not handle (a DNF type such as `(A&B)|null`,
    /// `static`), or names a class the request has not declared (an
    /// object cannot be of it yet; such writes keep the full check).
    #[cold]
    #[inline(never)]
    fn property_type_classes(
        &self,
        declared: &phera_bytecode::Type,
        property: &crate::classes::class::PropertyDefault,
    ) -> Option<Rc<ClassTargets>> {
        if declared.classes.is_empty() {
            return None;
        }
        let display = declared.display.trim_start_matches('?');
        if display.contains(['(', ')']) {
            return None;
        }
        let intersection = declared.intersection;
        let separator = if intersection { '&' } else { '|' };
        let declaring = self.classes.get(&property.declaring);
        let mut classes = Vec::new();
        for name in display.split(separator) {
            let class = match name.to_ascii_lowercase().as_str() {
                "null" | "false" | "true" | "bool" | "int" | "float" | "string" | "array"
                | "object" | "iterable" | "callable" | "mixed" => continue,
                "static" => return None,
                "self" => declaring.clone()?,
                "parent" => declaring.as_ref()?.parent.clone()?,
                _ => self.classes.get(name.trim_start_matches('\\'))?,
            };
            classes.push(class);
        }
        (!classes.is_empty()).then(|| {
            Rc::new(ClassTargets {
                classes: classes.into_boxed_slice(),
                intersection,
            })
        })
    }

    /// Mirrors validated write_place for mutable declarations and already matching
    /// values. Existing references retain write-through and type guards in the slow path.
    fn write_property_site(
        &mut self,
        place: &AccessPath,
        value: &Value,
        assignment: Option<(Reg, Reg)>,
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
        let widened = entry.write_rule.widened(value);
        let value = widened.as_ref().unwrap_or(value);
        let accepted = !matches!(value, Value::Reference(_)) && entry.write_rule.accepts(value);
        if !accepted {
            if entry.hooks.is_some() {
                return self.enter_cached_setter(&entry, object, value, assignment);
            }
            return self.enter_cached_magic_set(&entry, object, value, assignment);
        }
        if !crate::property_storage::write_direct_at(
            &object,
            &entry.key,
            entry.slot(),
            value,
            || {
                entry.write_rule.may_create()
                    && !has_magic_properties(&object)
                    && object.lazy().is_none()
            },
        ) {
            return Ok(false);
        }
        // The assignment's result is the value stored (`1.0` for a `float`
        // property assigned `1`).
        if let Some((_, result)) = assignment {
            self.set(result, value.clone());
        }
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
        match crate::property_storage::append_at(&object, &entry.key, entry.slot(), value) {
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
        assignment: Option<(Reg, Reg)>,
    ) -> Result<bool, NativeError> {
        if !matches!(place.steps()?.next(), Some(Step::Property(_))) {
            return Ok(false);
        }
        if self.write_property_site(place, value, assignment)? {
            return Ok(true);
        }
        let appended = self.append_property_site(place, value)?;
        if appended && let Some((_, result)) = assignment {
            self.set(result, value.clone());
        }
        Ok(appended)
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
