//! Optional activation state is stored beside the compact stack.
//!
//! `f(g(1))` has no extra state in either frame; `f` using `func_get_args()`
//! records its argument view at f's depth, without adding owners to g's frame.

/// Features consulted only by dynamic scope, iteration, methods or introspection.
pub(crate) struct FrameExtras {
    pub(crate) closure: Option<std::rc::Rc<phera_value::Object>>,
    pub(crate) method: Option<super::frame_scope::FrameMethod>,
    pub(crate) dynamic: crate::dynamic_variables::DynamicVariables,
    pub(crate) iterators: crate::iterators::Iterators,
    pub(crate) supplied_arguments: usize,
    pub(crate) saved_arguments: Option<usize>,
    pub(crate) argument_count: usize,
    pub(crate) extra_arguments: usize,
}

impl FrameExtras {
    /// Empty records own no PHP values; vectors grow only to the maximum PHP depth.
    pub(crate) fn new() -> Self {
        Self {
            closure: None,
            method: None,
            dynamic: crate::dynamic_variables::DynamicVariables::new(
                crate::dynamic_variables::VariableScope::Static,
            ),
            iterators: crate::iterators::Iterators::new(0),
            supplied_arguments: 0,
            saved_arguments: None,
            argument_count: 0,
            extra_arguments: 0,
        }
    }

    /// Release optional owners on return or unwind while keeping reusable storage.
    pub(crate) fn clear(&mut self) {
        self.closure = None;
        self.method = None;
        self.dynamic
            .reset(crate::dynamic_variables::VariableScope::Static);
        self.iterators.clear();
        self.saved_arguments = None;
        self.supplied_arguments = 0;
        self.argument_count = 0;
        self.extra_arguments = 0;
    }
}
