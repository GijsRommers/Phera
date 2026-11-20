//! The `Phera\snapshot()` function.
//!
//! ```php
//! var_dump(Phera\snapshot());   // phera script.php: bool(false), nothing kept
//!                               // phera serve: bool(true) in every request
//! ```
//!
//! In a boot run the call copies the VM as it is at that moment (the copy
//! is the snapshot, see [`super::Snapshot`]) and stops the run, so that
//! nothing after the call runs without a request. A copy that is refused
//! throws `Phera\SnapshotException` at the call, naming the class and where
//! the object was found; the boot then ends like any uncaught exception.

use phera_bytecode::Reg;
use phera_extension_api::{NativeError, NativeResult, Unwind};
use phera_value::Value;

use super::{Snapshot, SnapshotMode, SnapshotRefusal};
use crate::Vm;

/// The PHP class of a refused snapshot.
pub(crate) const EXCEPTION: &str = "Phera\\SnapshotException";

/// The payload that stops a boot run at the call. It is not an exception:
/// no `catch` or `finally` sees it, and no frame is unwound.
pub(crate) struct SnapshotStop;

impl Vm {
    /// `Phera\snapshot()`; `destination` is the register that receives its
    /// result, where a fork resumes.
    pub(crate) fn snapshot_intrinsic(
        &mut self,
        destination: Option<Reg>,
        mut arguments: Vec<Value>,
    ) -> NativeResult {
        phera_extension_api::prepare_call("Phera\\snapshot", &[], &mut arguments, self)?;
        match self.snapshot.mode {
            SnapshotMode::Off => Ok(Value::Bool(false)),
            SnapshotMode::Forked => Ok(Value::Bool(true)),
            SnapshotMode::Classic => {
                self.snapshot.called = true;
                Ok(Value::Bool(true))
            }
            SnapshotMode::Boot => {
                self.snapshot.resume = destination;
                // From here the classes are shared by every fork.
                self.classes.build_method_tables();
                let mut frozen = self.fork_vm().map_err(|refusal| {
                    let error = self.snapshot_exception(&refusal);
                    if let NativeError::Throw { message, .. } = &error {
                        self.snapshot.refused = Some(message.clone());
                    }
                    error
                })?;
                frozen.snapshot.mode = SnapshotMode::Forked;
                self.snapshot.taken = Some(Box::new(Snapshot { frozen }));
                Err(NativeError::Unwind(Unwind(Box::new(SnapshotStop))))
            }
        }
    }

    /// `Phera\SnapshotException: Phera\snapshot(): cannot keep PDO
    /// ($app->db->pdo): a connection in a transaction belongs to one request`.
    fn snapshot_exception(&self, refusal: &SnapshotRefusal) -> NativeError {
        let place = match self.path_to(refusal.address) {
            Some(path) => format!(" ({path})"),
            None => String::new(),
        };
        NativeError::Throw {
            class: EXCEPTION,
            message: format!(
                "Phera\\snapshot(): cannot keep {}{place}: {}",
                refusal.class, refusal.reason
            ),
            code: 0,
        }
    }
}
