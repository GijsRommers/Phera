//! `var_dump`, `print_r` and `var_export`: one walk over a value, three
//! output styles.
//!
//! The walk keeps its own stack instead of recursing. PHP code can nest
//! arrays as deep as memory allows, and a recursive printer would overflow
//! the thread's stack on such a value and take the whole server down.

mod print_r;
mod var_dump;
mod var_export;
mod walker;
use walker::walk;

use std::rc::Rc;

use phera_extension_api::value::Object;
use phera_extension_api::{ArrayKey, Context, PhpArray, PropertyPurpose, Registry, Value};

pub(crate) fn register(registry: &mut Registry) {
    var_dump::register(registry);
    print_r::register(registry);
    var_export::register(registry);
}

/// An array or object whose entries are being printed.
pub(crate) enum Container {
    Array(Rc<PhpArray>),
    Object(Rc<Object>),
}

impl Container {
    /// Distinguishes this container from every other one alive, so a value
    /// that contains itself (through a reference, or an object property) is
    /// noticed. Two containers alive at the same time never share an address.
    fn identity(&self) -> usize {
        match self {
            Container::Array(array) => Rc::as_ptr(array) as usize,
            Container::Object(object) => Rc::as_ptr(object) as usize,
        }
    }
}

/// Where a value sits in the output.
#[derive(Clone, Copy)]
pub(crate) struct Place {
    /// How many containers enclose it; 0 for the value being printed.
    pub depth: usize,
    /// Whether it is a PHP reference that something else still shares.
    pub is_reference: bool,
    pub leading_written: bool,
}

/// What differs between the three functions. The walker calls these in
/// document order.
pub(crate) trait Style {
    fn call_site(&self) -> (&'static str, &[Value]) {
        ("", &[])
    }

    fn before_debug(&mut self, _out: &mut Vec<u8>, _place: Place, _context: &mut dyn Context) {}

    fn property_purpose(&self) -> PropertyPurpose {
        PropertyPurpose::Debug
    }

    /// Anything that is not an array or object.
    fn scalar(&mut self, out: &mut Vec<u8>, value: &Value, place: Place, context: &mut dyn Context);

    fn uninitialized(&mut self, out: &mut Vec<u8>, ty: &str, depth: usize) {
        spaces(out, depth * 2);
        out.extend_from_slice(b"uninitialized(");
        out.extend_from_slice(ty.as_bytes());
        out.extend_from_slice(b")\n");
    }

    /// A case can be printed as one value or as a container of its readonly properties.
    fn enum_case(
        &mut self,
        _: &mut Vec<u8>,
        _: &Object,
        _: &phera_extension_api::PhpString,
        _: Place,
    ) -> bool {
        false
    }

    /// The start of an array or object with `len` entries.
    fn open(&mut self, out: &mut Vec<u8>, container: &Container, len: usize, place: Place);

    /// The key of the next entry. `depth` is the depth of the entry's value.
    fn key(&mut self, out: &mut Vec<u8>, key: &ArrayKey, container: &Container, depth: usize);

    /// After an entry's value, whatever it was.
    fn after_entry(&mut self, out: &mut Vec<u8>);

    /// The end of a container that was opened at `depth`.
    fn close(&mut self, out: &mut Vec<u8>, container: &Container, depth: usize);

    /// A container that is already being printed further up.
    fn recursion(
        &mut self,
        out: &mut Vec<u8>,
        container: &Container,
        place: Place,
        context: &mut dyn Context,
    );
}

pub(crate) fn spaces(out: &mut Vec<u8>, count: usize) {
    out.resize(out.len() + count, b' ');
}

#[cfg(test)]
mod tests {
    use phera_extension_api::testing::TestContext;

    use super::*;

    /// Counts what the walker visits and prints nothing. The real styles
    /// indent by depth, so their output for a deep value grows with the
    /// square of the depth and cannot be built in a test.
    #[derive(Default)]
    struct Tally {
        opened: usize,
        closed: usize,
        scalars: usize,
    }

    impl Style for Tally {
        fn scalar(
            &mut self,
            _out: &mut Vec<u8>,
            _value: &Value,
            _place: Place,
            _context: &mut dyn Context,
        ) {
            self.scalars += 1;
        }

        fn open(&mut self, _out: &mut Vec<u8>, _container: &Container, _len: usize, _place: Place) {
            self.opened += 1;
        }

        fn key(
            &mut self,
            _out: &mut Vec<u8>,
            _key: &ArrayKey,
            _container: &Container,
            _depth: usize,
        ) {
        }

        fn after_entry(&mut self, _out: &mut Vec<u8>) {}

        fn close(&mut self, _out: &mut Vec<u8>, _container: &Container, _depth: usize) {
            self.closed += 1;
        }

        fn recursion(
            &mut self,
            _out: &mut Vec<u8>,
            _container: &Container,
            _place: Place,
            _context: &mut dyn Context,
        ) {
        }
    }

    /// A recursive walk would need one stack frame per level and overflow
    /// long before 200,000 levels.
    #[test]
    fn walking_a_deeply_nested_array_does_not_use_the_stack() {
        let mut value = Value::Int(1);
        for _ in 0..200_000 {
            let mut array = PhpArray::new();
            array.insert(ArrayKey::int(0), value);
            value = Value::from(array);
        }
        let mut tally = Tally::default();
        {
            use std::{
                future::Future,
                task::{Context as TaskContext, Poll, Waker},
            };
            let mut out = Vec::new();
            let mut context = TestContext::default();
            let mut walk = std::pin::pin!(walk(&mut tally, &mut out, &value, &mut context));
            assert!(matches!(
                walk.as_mut()
                    .poll(&mut TaskContext::from_waker(Waker::noop())),
                Poll::Ready(Ok(()))
            ));
        }
        assert_eq!(
            (tally.opened, tally.closed, tally.scalars),
            (200_000, 200_000, 1)
        );
    }
}
