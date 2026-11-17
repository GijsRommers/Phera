//! Keep straight procedural scalar loops independent of the cold async dispatcher.

use crate::fast::{ScalarStep, scalar_step};
use phera_bytecode::Instruction;
use phera_value::Value;

/// How many loop rounds or calls a script may run before it lets other tasks
/// on the same thread have a turn.
///
/// Handing over is not free: the scheduler asks the operating system whether
/// any socket or timer is ready, which is a system call. At 4,096 that cost
/// 15 percent of a plain loop's time. 65,536 loop rounds are about a
/// millisecond, which is still far shorter than anyone waiting for a response
/// can notice.
pub(crate) const YIELD_AFTER_STEPS: usize = 65_536;

/// A batch stops before a PHP handler, at a cooperative yield, or at invalid code.
pub(crate) enum ScalarExit {
    Instruction(Instruction),
    Yield,
    Invalid,
}

/// Publish all progress before diagnostics, PHP calls, suspension or invalid-code errors.
pub(crate) struct ScalarBatch {
    pub(crate) ip: usize,
    pub(crate) executed: u64,
    pub(crate) work: usize,
    pub(crate) exit: ScalarExit,
}

/// Pure scalar steps run on a borrowed window until observable VM work is needed.
/// Keeping the counter local also avoids a VM-field write per scalar operation.
#[inline(never)]
pub(crate) fn run(
    instructions: &[Instruction],
    slots: &mut [Option<Value>],
    mut ip: usize,
    mut work: usize,
) -> ScalarBatch {
    let mut executed = 0;
    loop {
        let Some(instruction) = instructions.get(ip).copied() else {
            return ScalarBatch {
                ip,
                executed,
                work,
                exit: ScalarExit::Invalid,
            };
        };
        let previous_ip = ip;
        ip += 1;
        executed += 1;
        // The index arm handles an already verified integer
        // lookup, keeping adjacent scalar work in the same borrowed window.
        if scalar_step(instruction, slots, &mut ip) == ScalarStep::Fallback
            && !matches!(instruction, Instruction::FetchIndex { dst, base, index, .. }
                if crate::fast::array_index_step(dst, base, index, slots) == ScalarStep::Complete)
        {
            return ScalarBatch {
                ip,
                executed,
                work,
                exit: ScalarExit::Instruction(instruction),
            };
        }
        // Only backward edges contribute to the fairness budget. Comparing IPs
        // also covers fused conditional branches without another opcode match.
        if ip <= previous_ip {
            work += 1;
            if work >= YIELD_AFTER_STEPS {
                return ScalarBatch {
                    ip,
                    executed,
                    work: 0,
                    exit: ScalarExit::Yield,
                };
            }
        }
    }
}
