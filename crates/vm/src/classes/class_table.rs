//! Request class lookup, leased object ids, and weak cycle-cleanup records.

use std::{
    collections::HashMap,
    rc::{Rc, Weak},
};

use phera_value::{Object, ObjectIds};

use super::Class;
use super::method_table::MethodTable;
use super::operations::ClassOperations;

/// A class's position in the request's class table: a method frame's class
/// scope and called class are kept as these numbers.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ClassIndex(u32);

impl ClassIndex {
    pub(crate) const INVALID: Self = Self(u32::MAX);

    pub(crate) fn number(self) -> u32 {
        self.0
    }

    #[inline(always)]
    pub(crate) fn is_valid(self) -> bool {
        self.0 != u32::MAX
    }
}

/// Definitions linked once per worker for built-ins, or per request for PHP classes.
#[derive(Default, Clone)]
pub(crate) struct ClassDefinitions {
    /// Preserve registration/declaration order for get_declared_classes.
    pub(crate) order: Vec<String>,
    pub(crate) entries: HashMap<String, Rc<Class>>,
    /// Every class, by number.
    numbered: Vec<Rc<Class>>,
    closure: Option<ClassIndex>,
    pub(crate) has_property_views: bool,
    /// The built-in classes by the process key of their lowercased name, as
    /// class number plus one (0: none).
    by_key: Vec<u32>,
}

impl ClassDefinitions {
    /// Numbers the registered classes by name key (see `by_key`). The
    /// worker's built-ins are registered once, and this runs after them.
    pub(crate) fn number_keys(&mut self) {
        let names: Vec<(&String, u32)> = self
            .entries
            .iter()
            .map(|(name, class)| (name, class.index.0))
            .collect();
        let keys = phera_bytecode::name_keys::keys(names.iter().map(|(name, _)| name.as_str()));
        for ((_, number), key) in names.iter().zip(keys.iter()) {
            let slot = key.0 as usize;
            if self.by_key.len() <= slot {
                self.by_key.resize(slot + 1, 0);
            }
            self.by_key[slot] = number + 1;
        }
    }

    fn by_key(&self, key: phera_bytecode::NameKey) -> Option<&Rc<Class>> {
        let number = self.by_key.get(key.0 as usize)?.checked_sub(1)?;
        self.numbered.get(number as usize)
    }

    /// The number the next registered class gets.
    pub(crate) fn next_index(&self) -> ClassIndex {
        ClassIndex(self.numbered.len() as u32)
    }

    /// Register a class made with [`ClassTable::next_index`] under its
    /// lowercased name.
    pub(crate) fn insert(&mut self, lowercase_name: String, class: Rc<Class>) {
        self.has_property_views |= class
            .hooks
            .as_ref()
            .is_some_and(|hooks| hooks.properties.is_some());
        if class.origin == super::class::ClassOrigin::Closure {
            self.closure = Some(class.index);
        }
        self.numbered.push(class.clone());
        self.entries.insert(lowercase_name, class);
    }
}

// `Service::$instance = new Service` does not keep `Service` alive: the
// static's value lives in the request's `ClassStates`, not in the class, so
// no class owns an object that owns the class. That is also why dropping a
// fork's definitions must not touch its classes: the snapshot and the other
// forks share them.

/// One entry of a request's declaration order.
#[derive(Clone, Copy)]
enum Declared {
    /// The class at this position of the request's table.
    Class(u32),
    /// This request's `class_alias` at this index.
    Alias(u32),
}

/// One request's classes and objects. Built-ins are the worker's; PHP classes
/// are entries of the worker's table of linked classes (see
/// `super::worker_classes`), and the request holds which it has declared.
pub(crate) struct ClassTable {
    native: Rc<ClassDefinitions>,
    worker: Rc<super::worker_classes::WorkerClasses>,
    /// The entries this request has declared, by worker position: `None`
    /// where it has not. Whether a class exists in this request is this
    /// table's answer, never the worker's.
    local: Vec<Option<Rc<Class>>>,
    /// The worker positions of the declared classes, in declaration order.
    declared: Vec<u32>,
    /// What was declared, in order, aliases included (`get_declared_classes`):
    /// a position of `local` or an index of `aliases`, so that declaring a
    /// class copies no name.
    order: Vec<Declared>,
    /// This request's `class_alias` names, with their process key and the
    /// number of the class they name (which may be a built-in). Checked
    /// after the worker's names and the built-ins, so that a lookup of a
    /// declared class never walks it.
    aliases: Vec<(Rc<str>, phera_bytecode::NameKey, u32)>,
    /// The position reserved for the class being linked.
    linking: Option<u32>,
    /// While a class links: every class name it looked up, and what it was.
    recording: std::cell::RefCell<Option<Vec<super::worker_classes::Dependency>>>,
    /// The positions this request (last) and the snapshot it was forked
    /// from took for classes linked for themselves alone.
    leases: Vec<Rc<super::worker_classes::PositionLeases>>,
    pub(crate) has_dimensions: bool,
    pub(crate) has_property_views: bool,
    pub(crate) has_lazy_objects: bool,
    native_count: u32,
    pub(crate) objects: Vec<Weak<Object>>,
    ids: Rc<ObjectIds>,
    /// Allocations left before the cycle collector should run (see
    /// `crate::cycles`).
    allocations_before_collection: crate::cycles::Countdown,
    collection_requested: bool,
}

impl ClassTable {
    pub(crate) fn native_definitions(&self) -> Rc<ClassDefinitions> {
        self.native.clone()
    }

    pub(crate) fn new(
        native: Rc<ClassDefinitions>,
        worker: Rc<super::worker_classes::WorkerClasses>,
    ) -> Self {
        Self {
            leases: vec![super::worker_classes::PositionLeases::new(&worker)],
            worker,
            declared: Vec::new(),
            order: Vec::new(),
            aliases: Vec::new(),
            linking: None,
            recording: Default::default(),
            has_dimensions: false,
            has_property_views: native.has_property_views,
            has_lazy_objects: false,
            native_count: native.numbered.len() as u32,
            native,
            local: Vec::new(),
            objects: Vec::new(),
            ids: Rc::default(),
            allocations_before_collection: Default::default(),
            collection_requested: false,
        }
    }

    /// The number of the class being linked (see [`ClassTable::begin_link`]).
    pub(crate) fn next_index(&self) -> ClassIndex {
        ClassIndex(self.native_count + self.linking.unwrap_or(u32::MAX - self.native_count))
    }

    /// The worker's table of linked classes.
    pub(crate) fn worker(&self) -> Rc<super::worker_classes::WorkerClasses> {
        self.worker.clone()
    }

    /// Reserves the worker position of a class about to be linked, and
    /// starts recording the class names its linking looks up.
    pub(crate) fn begin_link(&mut self) {
        self.linking = Some(self.worker.reserve());
        *self.recording.borrow_mut() = Some(Vec::new());
    }

    /// Linking against a class that is still being declared is request-local.
    pub(crate) fn discard_link_dependencies(&self) {
        *self.recording.borrow_mut() = None;
    }

    /// Ends a link: the reusable dependencies, and the position it used.
    pub(crate) fn end_link(
        &mut self,
    ) -> (Option<Vec<super::worker_classes::Dependency>>, Option<u32>) {
        let recorded = self.recording.borrow_mut().take();
        (recorded, self.linking.take())
    }

    /// Gives `position` back to the worker when this request (and every fork
    /// of it) is gone: a class linked for this request alone, or a link
    /// that failed.
    pub(crate) fn lease_position(&self, position: u32) {
        if let Some(own) = self.leases.last() {
            own.lease(position);
        }
    }

    /// The number of the class `key` names in this request, without
    /// recording the lookup.
    pub(crate) fn resolve_number(&self, key: phera_bytecode::NameKey) -> Option<u32> {
        self.by_key(key).map(|class| class.index.0)
    }

    /// Names a class linked for this request alone: found through its own
    /// names, not the worker's.
    pub(crate) fn own_name(&mut self, lowercase: &str, position: u32) {
        self.worker.chain_leased(lowercase, position);
    }

    /// The class a name of the running code means in this request, by the
    /// name's process key: `new Post`, `Post::query()`, `instanceof Post`.
    #[inline]
    pub(crate) fn by_key(&self, key: phera_bytecode::NameKey) -> Option<&Rc<Class>> {
        if let Some(found) = self.declared_by_key(key) {
            return Some(found);
        }
        if let Some(found) = self.native.by_key(key) {
            return Some(found);
        }
        let (_, _, number) = self.aliases.iter().find(|(_, alias, _)| *alias == key)?;
        self.by_number(*number)
    }

    /// The class this request declared among the worker's entries named by
    /// `key`.
    #[inline]
    fn declared_by_key(&self, key: phera_bytecode::NameKey) -> Option<&Rc<Class>> {
        let mut position = self.worker.first(key);
        while let Some(at) = position {
            if let Some(Some(class)) = self.local.get(at as usize) {
                return Some(class);
            }
            position = self.worker.after(at);
        }
        None
    }

    /// The class a lowercased name means in this request, from a string a
    /// script built or a declaration names.
    fn find(&self, lowercase: &str) -> Option<&Rc<Class>> {
        if let Some(found) = self
            .worker
            .key_of(lowercase)
            .and_then(|key| self.declared_by_key(key))
        {
            return Some(found);
        }
        if let Some(found) = self.native.entries.get(lowercase) {
            return Some(found);
        }
        let (_, _, number) = self
            .aliases
            .iter()
            .find(|(alias, _, _)| &**alias == lowercase)?;
        self.by_number(*number)
    }

    /// The class this request linked at worker `position`.
    pub(crate) fn at_position(&self, position: u32) -> Option<Rc<Class>> {
        self.local.get(position as usize)?.clone()
    }

    /// Declares a class another request of the worker linked.
    pub(crate) fn declare_entry(&mut self, class: Rc<Class>) {
        let position = class.index.0 - self.native_count;
        self.register(position, class);
    }

    fn register(&mut self, position: u32, class: Rc<Class>) {
        self.has_property_views |= class
            .hooks
            .as_ref()
            .is_some_and(|hooks| hooks.properties.is_some());
        let index = position as usize;
        if self.local.len() <= index {
            self.local.resize(index + 1, None);
        }
        self.order.push(Declared::Class(position));
        self.local[index] = Some(class);
        self.declared.push(position);
    }

    /// The class's methods by name key, built once on the class itself and
    /// shared across every request and fork in the worker (native and PHP alike).
    #[inline(always)]
    pub(crate) fn method_table<'c>(&'c self, class: &'c Class) -> &'c MethodTable {
        class
            .method_table
            .get_or_init(|| MethodTable::build(class, |method| self.get(&method.declaring)))
    }

    #[inline(always)]
    pub(crate) fn cached_method_table<'c>(&'c self, class: &'c Class) -> Option<&'c MethodTable> {
        class.method_table.get()
    }

    /// Builds the flat method, property, constant and static-property tables of
    /// every native and user class of this request that has none yet. Taking a
    /// snapshot calls this so that no fork ever pays to build them.
    pub(crate) fn build_method_tables(&self) {
        for class in self.native.numbered.iter().chain(self.local_classes()) {
            class
                .method_table
                .get_or_init(|| MethodTable::build(class, |method| self.get(&method.declaring)));
            let _ = class.property_table();
            let _ = class.constant_table();
            let _ = class.static_prop_table();
        }
    }

    /// Declares the class this request just linked, at its reserved position.
    pub(crate) fn insert(&mut self, class: Rc<Class>) {
        let position = class.index.0 - self.native_count;
        self.register(position, class);
    }

    /// A declaration completed across an autoload is request-local.
    pub(crate) fn publish_delayed(&mut self, class: Rc<Class>) {
        let position = class.index.0 - self.native_count;
        self.own_name(&class.name.to_ascii_lowercase(), position);
        self.insert(class);
    }

    pub(crate) fn alias(&mut self, name: String, class: Rc<Class>) -> bool {
        if self.get_lowercase(&name).is_some() {
            return false;
        }
        self.order.push(Declared::Alias(self.aliases.len() as u32));
        let key = phera_bytecode::name_keys::key(&name);
        self.aliases.push((name.into(), key, class.index.0));
        true
    }

    /// The request's own classes, in declaration order.
    pub(crate) fn local_classes(&self) -> impl Iterator<Item = &Rc<Class>> {
        self.declared
            .iter()
            .filter_map(|position| self.local.get(*position as usize)?.as_ref())
    }

    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.native
            .order
            .iter()
            .map(String::as_str)
            .chain(self.order.iter().filter_map(|declared| {
                match declared {
                    Declared::Class(position) => self
                        .local
                        .get(*position as usize)?
                        .as_ref()
                        .map(|class| class.name.as_str()),
                    Declared::Alias(index) => {
                        self.aliases.get(*index as usize).map(|alias| &*alias.0)
                    }
                }
            }))
    }

    /// The class with number `index`.
    #[inline]
    pub(crate) fn class(&self, index: ClassIndex) -> &Rc<Class> {
        let index = index.0 as usize;
        let base = self.native_count as usize;
        if index < base {
            &self.native.numbered[index]
        } else {
            self.local[index - base]
                .as_ref()
                .expect("a class number of this request names a declared class")
        }
    }

    pub(crate) fn numbered(&self, index: u32) -> Option<Rc<Class>> {
        let index = index as usize;
        let base = self.native_count as usize;
        if index < base {
            self.native.numbered.get(index).cloned()
        } else {
            self.local.get(index - base)?.clone()
        }
    }

    pub(crate) fn by_number(&self, number: u32) -> Option<&Rc<Class>> {
        let number = number as usize;
        let base = self.native_count as usize;
        match number.checked_sub(base) {
            Some(position) => self.local.get(position)?.as_ref(),
            None => self.native.numbered.get(number),
        }
    }

    /// The built-in Closure class is fixed for a request.
    pub(crate) fn closure_class(&self) -> Option<Rc<Class>> {
        let index = self.native.closure?;
        self.native.numbered.get(index.0 as usize).cloned()
    }

    pub(crate) fn get_lowercase(&self, name: &str) -> Option<&Rc<Class>> {
        let found = self.find(name);
        if let Ok(mut recording) = self.recording.try_borrow_mut()
            && let Some(recorded) = recording.as_mut()
        {
            // Recording runs while a class is linked, not on a reuse.
            let key = phera_bytecode::name_keys::key(name);
            recorded.push((key, found.map(|class| class.index.0)));
        }
        found
    }

    /// Class identifiers ignore case, including names produced by dynamic expressions.
    pub(crate) fn get(&self, name: &str) -> Option<Rc<Class>> {
        self.get_ref(name).cloned()
    }

    /// Borrowed lookup matching [`Self::get`], avoiding an `Rc::clone` on fast paths.
    pub(crate) fn get_ref(&self, name: &str) -> Option<&Rc<Class>> {
        let name = name.strip_prefix('\\').unwrap_or(name);
        if !name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return self.get_lowercase(name);
        }
        let mut buffer = [0; 64];
        if let Some(bytes) = buffer.get_mut(..name.len()) {
            bytes.copy_from_slice(name.as_bytes());
            bytes.make_ascii_lowercase();
            if let Ok(name) = std::str::from_utf8(bytes) {
                return self.get_lowercase(name);
            }
        }
        self.get_lowercase(&name.to_ascii_lowercase())
    }

    /// Value-owned leases implement PHP's LIFO reuse without inspecting weak handles.
    pub(crate) fn allocate(&mut self, class: Rc<Class>) -> Rc<Object> {
        let properties = phera_value::Properties::for_class(&*class);
        self.allocate_with_properties(class, properties)
    }

    pub(crate) fn allocate_with_properties(
        &mut self,
        class: Rc<Class>,
        properties: phera_value::Properties,
    ) -> Rc<Object> {
        self.has_dimensions |= class.operations.has(ClassOperations::DIMENSIONS);
        let has_destructor = class
            .magic
            .as_ref()
            .is_some_and(|magic| magic.destruct.is_some());
        let object = Object::allocate_with(&self.ids, class, properties);
        if has_destructor {
            object.expect_destructor();
        }
        let object = Rc::new(object);
        self.track(&object);
        self.count_allocation();
        object
    }

    /// One more allocation towards the next automatic cycle collection.
    /// When it is due, the interpreter's check for due destructors fires.
    #[inline]
    fn count_allocation(&mut self) {
        if self.allocations_before_collection.tick() {
            self.collection_requested = true;
            self.ids.wake_interpreter();
        }
    }

    /// Whether a collection was asked for; clears the request.
    pub(crate) fn take_collection_request(&mut self) -> bool {
        std::mem::take(&mut self.collection_requested)
    }

    /// The next automatic collection comes after `allocations` more objects.
    pub(crate) fn request_collection_after(&mut self, allocations: usize) {
        self.allocations_before_collection = crate::cycles::Countdown::new(allocations);
    }

    /// Every live object, each handle made as it is taken, for the cycle
    /// collector.
    pub(crate) fn live_objects(&self) -> impl Iterator<Item = Rc<Object>> + '_ {
        self.objects.iter().filter_map(Weak::upgrade)
    }

    /// Records a live object under its id, for the end of the request: its
    /// destructor phase and the cleanup of cycles. No weak handle's liveness
    /// takes part in allocation or id selection. Always inlined: it is part
    /// of every allocation.
    #[inline(always)]
    pub(crate) fn track(&mut self, object: &Rc<Object>) {
        let index = object.id.saturating_sub(1) as usize;
        if let Some(slot) = self.objects.get_mut(index) {
            *slot = Rc::downgrade(object);
        } else {
            self.objects.resize_with(index, Weak::new);
            self.objects.push(Rc::downgrade(object));
        }
    }

    /// A handle on the request's object ids, for the interpreter loop.
    #[inline]
    pub(crate) fn ids_handle(&self) -> Rc<ObjectIds> {
        self.ids.clone()
    }

    /// The request's object ids, which also hold the objects whose
    /// destructor is due.
    #[inline]
    pub(crate) fn ids(&self) -> &ObjectIds {
        &self.ids
    }
}

impl Drop for ClassTable {
    /// Break request-local object cycles without retaining objects during normal execution.
    fn drop(&mut self) {
        // A request that ended without its destructor phase (stopped by the
        // server's timeout) frees its objects without destructors. Closing
        // the pool first matters: an object queued for its destructor holds
        // the pool, which holds the queue, so an unclosed queue would leak.
        drop(self.ids.close());
        for object in self.objects.iter().filter_map(Weak::upgrade) {
            object.update(|properties| *properties = phera_value::Properties::default());
            drop(object.take_attachments());
            if let Ok(mut native) = object.native.try_borrow_mut() {
                *native = None;
            }
        }
    }
}

impl ClassTable {
    /// The class table of a fork: the same classes (immutable, shared), the
    /// method tables built so far (they use name numbers the fork shares),
    /// and the fork's own objects and id pool.
    pub(crate) fn forked(&self, ids: Rc<ObjectIds>, objects: Vec<Weak<Object>>) -> ClassTable {
        ClassTable {
            native: self.native.clone(),
            worker: self.worker.clone(),
            local: self.local.clone(),
            declared: self.declared.clone(),
            order: self.order.clone(),
            aliases: self.aliases.clone(),
            linking: None,
            recording: Default::default(),
            leases: self
                .leases
                .iter()
                .cloned()
                .chain([super::worker_classes::PositionLeases::new(&self.worker)])
                .collect(),
            has_dimensions: self.has_dimensions,
            has_property_views: self.has_property_views,
            has_lazy_objects: self.has_lazy_objects,
            native_count: self.native_count,
            objects,
            ids,
            allocations_before_collection: self.allocations_before_collection.clone(),
            collection_requested: false,
        }
    }

    /// Mirrors name lookup for external handles, while linked VM classes carry
    /// a weak self identity whose owner is already kept alive by the object.
    pub(crate) fn for_object(&self, object: &Object) -> Option<Rc<Class>> {
        object
            .class
            .as_any()
            .downcast_ref::<Class>()
            .and_then(|class| class.identity.upgrade())
            .or_else(|| self.get(object.class.name()))
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;
    use std::sync::Arc;

    use phera_extension_api::Registry;
    use phera_value::{ArrayKey, Value};

    use super::{ClassDefinitions, ClassTable};

    #[tokio::test]
    async fn static_singletons_and_cached_enum_cases_release_their_classes() {
        let source = "<?php
            class Singleton { public static $instance; }
            Singleton::$instance = new Singleton;
            enum Token { case Only; const ALIAS = self::Only; }
            echo Token::ALIAS->name;";
        let unit = Arc::new(phera_compiler::compile(source, "class-cycles.php").unwrap());
        let mut vm = crate::Vm::new(Arc::new(Registry::new()));
        assert_eq!(vm.run(unit).await, 0);
        assert_eq!(vm.output, b"Only");
        let singleton = Rc::downgrade(&vm.classes.get("Singleton").unwrap());
        let token = Rc::downgrade(&vm.classes.get("Token").unwrap());
        drop(vm);
        assert_eq!(singleton.strong_count(), 0);
        assert_eq!(token.strong_count(), 0);
    }

    #[test]
    fn freed_object_ids_reuse_the_most_recently_freed_slot() {
        let mut table = ClassTable::new(
            Rc::new(ClassDefinitions::native(&Registry::new())),
            Rc::default(),
        );
        let class = table.get("stdClass").unwrap();
        let first = table.allocate(class.clone());
        let second = table.allocate(class.clone());
        assert_eq!((first.id, second.id), (1, 2));
        drop(first);
        let replacement = table.allocate(class);
        assert_eq!(replacement.id, 1);
        assert_eq!(Rc::strong_count(&second), 1);
        let third = table.allocate(table.get("stdClass").unwrap());
        assert_eq!(third.id, 3);
        drop(replacement);
        drop(second);
        let class = table.get("stdClass").unwrap();
        let reused_second = table.allocate(class.clone());
        let reused_first = table.allocate(class.clone());
        let next = table.allocate(class);
        assert_eq!((reused_second.id, reused_first.id, next.id), (2, 1, 4));
    }

    #[test]
    fn a_million_linked_objects_drop_without_rust_recursion() {
        let table = ClassTable::new(
            Rc::new(ClassDefinitions::native(&Registry::new())),
            Rc::default(),
        );
        let class = table.get("stdClass").unwrap();
        let mut head = Value::Null;
        for index in 0..1_000_000 {
            let object = Rc::new(phera_value::Object::new(index + 1, class.clone()));
            object.set(ArrayKey::str("next"), head);
            head = Value::Object(object);
        }
        drop(head);
    }
}
