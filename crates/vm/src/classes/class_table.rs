//! Request class lookup, leased object ids, and weak cycle-cleanup records.

use std::{
    cell::OnceCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

use phera_value::{Object, ObjectIds, PhpArray};

use super::Class;
use super::method_table::MethodTable;
use super::operations::ClassOperations;
use crate::code_table::CodeTable;

type NativeMethodTables = Box<[OnceCell<MethodTable>]>;

/// A class's position in the request's class table: a method frame's class
/// scope and called class are kept as these numbers.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub(crate) struct ClassIndex(u32);

impl ClassIndex {
    pub(crate) fn number(self) -> u32 {
        self.0
    }
}

/// Definitions linked once per worker for built-ins, or per request for PHP classes.
#[derive(Default)]
pub(crate) struct ClassDefinitions {
    /// Preserve registration/declaration order for get_declared_classes.
    pub(crate) order: Vec<String>,
    pub(crate) entries: HashMap<String, Rc<Class>>,
    /// Every class, by number.
    numbered: Vec<Rc<Class>>,
    closure: Option<ClassIndex>,
    pub(crate) has_property_views: bool,
}

impl ClassDefinitions {
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

impl Drop for ClassDefinitions {
    fn drop(&mut self) {
        // A static singleton or a cached enum case can keep its own class
        // alive. Release those PHP values before dropping the class owners.
        for class in &self.numbered {
            class.release_values();
        }
    }
}

/// One request owns its class additions and objects. Built-ins keep their indices on the worker.
pub(crate) struct ClassTable {
    native: Rc<ClassDefinitions>,
    local: ClassDefinitions,
    pub(crate) has_dimensions: bool,
    pub(crate) has_property_views: bool,
    pub(crate) has_lazy_objects: bool,
    native_count: u32,
    native_methods: OnceCell<NativeMethodTables>,
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

    pub(crate) fn new(native: Rc<ClassDefinitions>) -> Self {
        Self {
            has_dimensions: false,
            has_property_views: native.has_property_views,
            has_lazy_objects: false,
            native_count: native.numbered.len() as u32,
            native_methods: OnceCell::new(),
            native,
            local: ClassDefinitions::default(),
            objects: Vec::new(),
            ids: Rc::default(),
            allocations_before_collection: Default::default(),
            collection_requested: false,
        }
    }

    pub(crate) fn next_index(&self) -> ClassIndex {
        ClassIndex((self.native_count as usize + self.local.numbered.len()) as u32)
    }

    /// `$reflection->getName()` uses this request's name numbers, even when
    /// the class definition is shared with another request on this worker.
    #[inline]
    pub(crate) fn method_table<'c>(
        &'c self,
        class: &'c Class,
        code: &CodeTable,
    ) -> &'c MethodTable {
        let index = class.index.0 as usize;
        let cell = if index < self.native_count as usize {
            &self
                .native_methods
                .get()
                .unwrap_or_else(|| self.allocate_native_method_tables())[index]
        } else {
            &class.method_table
        };
        cell.get_or_init(|| MethodTable::build(class, code, |method| self.get(&method.declaring)))
    }

    #[cold]
    fn allocate_native_method_tables(&self) -> &NativeMethodTables {
        self.native_methods
            .get_or_init(|| (0..self.native_count).map(|_| OnceCell::new()).collect())
    }

    pub(crate) fn cached_method_table<'c>(&'c self, class: &'c Class) -> Option<&'c MethodTable> {
        let index = class.index.0 as usize;
        if index < self.native_count as usize {
            self.native_methods.get()?[index].get()
        } else {
            class.method_table.get()
        }
    }

    pub(crate) fn insert(&mut self, lowercase_name: String, class: Rc<Class>) {
        debug_assert_eq!(class.index, self.next_index());
        self.local.order.push(class.name.clone());
        self.local.insert(lowercase_name, class);
        self.has_property_views |= self.local.has_property_views;
    }

    pub(crate) fn alias(&mut self, name: String, class: Rc<Class>) -> bool {
        if self.get_lowercase(&name).is_some() {
            return false;
        }
        self.local.order.push(name.clone());
        self.local.entries.insert(name, class);
        true
    }

    pub(crate) fn names(&self) -> impl Iterator<Item = &str> {
        self.native
            .order
            .iter()
            .chain(&self.local.order)
            .map(String::as_str)
    }

    /// The class with number `index`.
    #[inline]
    pub(crate) fn class(&self, index: ClassIndex) -> &Rc<Class> {
        let index = index.0 as usize;
        let base = self.native_count as usize;
        if index < base {
            &self.native.numbered[index]
        } else {
            &self.local.numbered[index - base]
        }
    }

    pub(crate) fn numbered(&self, index: u32) -> Option<Rc<Class>> {
        let index = index as usize;
        let base = self.native_count as usize;
        if index < base {
            self.native.numbered.get(index).cloned()
        } else {
            self.local.numbered.get(index - base).cloned()
        }
    }

    /// The built-in Closure class is fixed for a request.
    pub(crate) fn closure_class(&self) -> Option<Rc<Class>> {
        let index = self.native.closure?;
        self.native.numbered.get(index.0 as usize).cloned()
    }

    pub(crate) fn get_lowercase(&self, name: &str) -> Option<&Rc<Class>> {
        self.local
            .entries
            .get(name)
            .or_else(|| self.native.entries.get(name))
    }

    /// Class identifiers ignore case, including names produced by dynamic expressions.
    pub(crate) fn get(&self, name: &str) -> Option<Rc<Class>> {
        let name = name.strip_prefix('\\').unwrap_or(name);
        if !name.bytes().any(|byte| byte.is_ascii_uppercase()) {
            return self.get_lowercase(name).cloned();
        }
        let mut buffer = [0; 64];
        if let Some(bytes) = buffer.get_mut(..name.len()) {
            bytes.copy_from_slice(name.as_bytes());
            bytes.make_ascii_lowercase();
            if let Ok(name) = std::str::from_utf8(bytes) {
                return self.get_lowercase(name).cloned();
            }
        }
        self.get_lowercase(&name.to_ascii_lowercase()).cloned()
    }

    /// Value-owned leases implement PHP's LIFO reuse without inspecting weak handles.
    pub(crate) fn allocate(&mut self, class: Rc<Class>) -> Rc<Object> {
        self.allocate_with_properties(class, PhpArray::new())
    }

    pub(crate) fn allocate_with_properties(
        &mut self,
        class: Rc<Class>,
        properties: PhpArray,
    ) -> Rc<Object> {
        self.has_dimensions |= class.operations.has(ClassOperations::DIMENSIONS);
        let has_destructor = class
            .magic
            .as_ref()
            .is_some_and(|magic| magic.destruct.is_some());
        let object = Object::allocate_with_properties(&self.ids, class, properties);
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
            object.update(|properties| *properties = phera_value::PhpArray::new());
            drop(object.take_attachments());
            if let Ok(mut native) = object.native.try_borrow_mut() {
                *native = None;
            }
        }
    }
}

impl ClassTable {
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
        let mut table = ClassTable::new(Rc::new(ClassDefinitions::native(&Registry::new())));
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
        let table = ClassTable::new(Rc::new(ClassDefinitions::native(&Registry::new())));
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
