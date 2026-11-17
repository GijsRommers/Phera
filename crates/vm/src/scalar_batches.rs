//! Keep straight procedural scalar loops independent of the cold async dispatcher.

use crate::fast::{ArgumentTracking, ScalarStep, scalar_step};
use phera_bytecode::Instruction;
use phera_value::Value;

/// A batch stops before a PHP handler, at a cooperative yield, or at invalid code.
pub(crate) enum BatchExit {
    Instruction(Instruction),
    Yield,
    Invalid,
}

/// Publish all progress before diagnostics, PHP calls, suspension or invalid-code errors.
pub(crate) struct ScalarBatch {
    pub(crate) ip: usize,
    pub(crate) executed: u64,
    pub(crate) work: usize,
    pub(crate) exit: BatchExit,
}

/// Mirrors the interpreter's scalar loop. Only already-supported scalar steps
/// execute here; no PHP call or fallible PHP coercion runs on the Rust stack.
/// Keeping the counter local also avoids a VM-field write per scalar operation.
#[inline(never)]
pub(crate) fn run(
    instructions: &[Instruction],
    slots: &mut [Option<Value>],
    mut ip: usize,
    mut work: usize,
    tracking: ArgumentTracking,
) -> ScalarBatch {
    let mut executed = 0;
    loop {
        let Some(instruction) = instructions.get(ip).copied() else {
            return ScalarBatch {
                ip,
                executed,
                work,
                exit: BatchExit::Invalid,
            };
        };
        let previous_ip = ip;
        ip += 1;
        executed += 1;
        // The index arm mirrors the general interpreter's verified integer
        // lookup, keeping adjacent scalar work in the same borrowed window.
        if scalar_step(instruction, slots, &mut ip, tracking) == ScalarStep::Fallback
            && !matches!(instruction, Instruction::FetchIndex { dst, base, index, .. }
                if crate::fast::array_index_step(dst, base, index, slots) == ScalarStep::Complete)
        {
            return ScalarBatch {
                ip,
                executed,
                work,
                exit: BatchExit::Instruction(instruction),
            };
        }
        // Only backward edges contribute to the fairness budget. Comparing IPs
        // also covers fused conditional branches without another opcode match.
        if ip <= previous_ip {
            work += 1;
            if work >= 4096 {
                return ScalarBatch {
                    ip,
                    executed,
                    work: 0,
                    exit: BatchExit::Yield,
                };
            }
        }
    }
}
