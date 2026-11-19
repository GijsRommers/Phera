//! If a timeout follows `$before = new Resource;`, PHP skips its destructor.
//! Objects kept alive by a shutdown callback can still be destroyed before
//! output buffers are flushed.

use std::rc::Weak;

use phera_value::Object;

use crate::Vm;

impl Vm {
    pub(crate) fn mark_timed_out_objects(&mut self) {
        self.timed_out_objects = Some(self.classes.objects.clone());
    }

    /// Globals outlive the abandoned main frame and remain visible in shutdown.
    pub(crate) fn retain_timeout_globals(&mut self) {
        let Some(main) = self.global_frame() else {
            return;
        };
        let slots = main.slots(&self.registers);
        let values: Vec<_> = main
            .named_registers()
            .map(|(name, register)| (name.clone(), slots.get(register.index()).cloned().flatten()))
            .collect();
        for (name, value) in values {
            if let Some(value) = value {
                self.globals.insert(name, value);
            } else {
                self.globals.remove(&name);
            }
        }
    }

    pub(crate) async fn destroy_objects_after_timeout(&mut self, before: Vec<Weak<Object>>) {
        let mut index = 0;
        while let Some(current) = self.classes.objects.get(index) {
            let original = before.get(index);
            index += 1;
            if original.is_some_and(|original| original.ptr_eq(current)) {
                continue;
            }
            let Some(object) = current.upgrade() else {
                continue;
            };
            if object.is_uninitialized_lazy() {
                continue;
            }
            let Some(class) = self.classes.for_object(&object) else {
                continue;
            };
            let Some(method) = class.methods.get("__destruct").cloned() else {
                continue;
            };
            let visibility = match method.visibility {
                phera_bytecode::Visibility::Public => {
                    if let Err(error) = self.invoke_magic_method(method, class, object, []).await {
                        self.report_uncaught(error).await;
                        break;
                    }
                    continue;
                }
                phera_bytecode::Visibility::Protected => "protected",
                phera_bytecode::Visibility::Private => "private",
            };
            self.warn(format!(
                "Call to {visibility} {}::__destruct() from global scope during shutdown ignored",
                class.name
            ));
            if let Err(error) = self.drain_error_handlers().await {
                self.report_uncaught(error).await;
                break;
            }
        }
    }
}
