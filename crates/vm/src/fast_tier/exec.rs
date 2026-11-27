//! Tier-2 compact register VM execution engine (100% safe Rust).
//!
//! Runs `FastOp` streams over fixed 256-slot `&mut [u8; 256]` + `&mut [u64; 256]`
//! windows (zero bounds checks by type), with in-loop numeric float/int ops,
//! fused loop latches, direct 1D/2D `PhpArray` ops, and in-loop `CallFast` /
//! `Return` frames. Any unhandled case deoptimizes cleanly before mutating
//! observable state.

use std::rc::Rc;

use phera_bytecode::{Reg, TypeMask};
use phera_value::{PhpArray, Value};

use crate::calls::call_stack::CallStack;
use crate::calls::execution_loop::{Position, YIELD_AFTER_STEPS};
use crate::calls::frame::Frame;
use crate::calls::prepared_call::CallKind;
use crate::calls::registers::Registers;
use crate::code_table::{CodeTable, FunctionIndex};
use crate::fast_tier::stack::{FastFrame, FastStack};
use crate::fast_tier::{
    CMP_EQ, CMP_GE, CMP_GT, CMP_IDENT, CMP_LE, CMP_LT, CMP_NE, CMP_NOT_IDENT, FastEntry, FastExit,
    FastFunction, Op, TAG_BOOL, TAG_FLOAT, TAG_HEAP, TAG_INT, TAG_NULL, TAG_UNINIT,
};

#[inline(always)]
fn tag_type_mask(tag: u8, bit: u64, heap: &[Option<Value>]) -> Option<TypeMask> {
    match tag {
        TAG_NULL => Some(TypeMask::NULL),
        TAG_BOOL => Some(if bit != 0 {
            TypeMask::TRUE
        } else {
            TypeMask::FALSE
        }),
        TAG_INT => Some(TypeMask::INT),
        TAG_FLOAT => Some(TypeMask::FLOAT),
        TAG_HEAP => {
            let val = heap.get(bit as usize)?.as_ref()?;
            if val.is_reference() {
                None
            } else {
                Some(crate::types::own_type(val))
            }
        }
        _ => None,
    }
}

#[inline(always)]
fn return_mask_fits(mask: TypeMask, tag: u8, bit: u64, heap: &[Option<Value>]) -> bool {
    if tag == TAG_NULL && mask.intersects(TypeMask::VOID) {
        return true;
    }
    tag_type_mask(tag, bit, heap).is_some_and(|own| mask.intersects(own))
}

#[inline(always)]
fn scalar_truthy(tag: u8, bit: u64) -> Option<bool> {
    match tag {
        TAG_NULL => Some(false),
        TAG_BOOL => Some(bit != 0),
        TAG_INT => Some(bit != 0),
        TAG_FLOAT => Some(f64::from_bits(bit) != 0.0),
        _ => None,
    }
}

#[inline(always)]
fn compare_nums(cmp: u8, lt: u8, lb: u64, rt: u8, rb: u64) -> Option<bool> {
    if lt == TAG_INT && rt == TAG_INT {
        let a = lb as i64;
        let b = rb as i64;
        return Some(match cmp {
            CMP_EQ | CMP_IDENT => a == b,
            CMP_NE | CMP_NOT_IDENT => a != b,
            CMP_LT => a < b,
            CMP_LE => a <= b,
            CMP_GT => a > b,
            CMP_GE => a >= b,
            _ => return None,
        });
    }
    if (lt == TAG_INT || lt == TAG_FLOAT) && (rt == TAG_INT || rt == TAG_FLOAT) {
        if cmp == CMP_IDENT {
            return Some(lt == rt && f64::from_bits(lb) == f64::from_bits(rb));
        }
        if cmp == CMP_NOT_IDENT {
            return Some(lt != rt || f64::from_bits(lb) != f64::from_bits(rb));
        }
        let a = if lt == TAG_FLOAT {
            f64::from_bits(lb)
        } else {
            (lb as i64) as f64
        };
        let b = if rt == TAG_FLOAT {
            f64::from_bits(rb)
        } else {
            (rb as i64) as f64
        };
        return Some(match cmp {
            CMP_EQ => a == b,
            CMP_NE => a != b,
            CMP_LT => a < b,
            CMP_LE => a <= b,
            CMP_GT => a > b,
            CMP_GE => a >= b,
            _ => return None,
        });
    }
    if (cmp == CMP_IDENT || cmp == CMP_NOT_IDENT)
        && lt != TAG_UNINIT
        && rt != TAG_UNINIT
        && lt != TAG_HEAP
        && rt != TAG_HEAP
    {
        let eq = lt == rt
            && match lt {
                TAG_NULL => true,
                TAG_BOOL | TAG_INT => lb == rb,
                TAG_FLOAT => f64::from_bits(lb) == f64::from_bits(rb),
                _ => false,
            };
        return Some(if cmp == CMP_IDENT { eq } else { !eq });
    }
    None
}

/// Write a scalar `(tag, bit)` into register `dst`. If `dst` currently holds
/// `TAG_HEAP`, refuses to overwrite a PHP reference (returns `false` -> deopt)
/// and otherwise drops the previous heap slot.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn write_scalar(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
    local_count: u8,
    dst: usize,
    tag: u8,
    bit: u64,
) -> bool {
    if t[dst] == TAG_HEAP {
        let idx = v[dst] as u32;
        if dst < local_count as usize
            && heap
                .get(idx as usize)
                .and_then(Option::as_ref)
                .is_some_and(Value::is_reference)
        {
            return false;
        }
        if let Some(slot) = heap.get_mut(idx as usize) {
            *slot = None;
            free_heap.push(idx);
        }
    }
    t[dst] = tag;
    v[dst] = bit;
    true
}

/// Write a heap `Value` into register `dst`, refusing to overwrite a PHP
/// reference in a named local.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn write_heap(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    local_count: u8,
    has_heap: &mut bool,
    dst: usize,
    value: Value,
) -> bool {
    if t[dst] == TAG_HEAP {
        let idx = v[dst] as usize;
        if dst < local_count as usize
            && heap
                .get(idx)
                .and_then(Option::as_ref)
                .is_some_and(Value::is_reference)
        {
            return false;
        }
        if let Some(slot) = heap.get_mut(idx) {
            *slot = Some(value);
            *has_heap = true;
            return true;
        }
    }
    let idx = if let Some(slot) = free_heap.pop() {
        heap[slot as usize] = Some(value);
        slot
    } else {
        let slot = heap.len() as u32;
        heap.push(Some(value));
        slot
    };
    t[dst] = TAG_HEAP;
    v[dst] = u64::from(idx);
    *has_heap = true;
    true
}

/// Store a `&Value` returned by an array lookup into `dst`, keeping scalars
/// completely unboxed in `(t, v)` and cloning only non-reference heap values.
#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn store_borrowed_value(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    local_count: u8,
    has_heap: &mut bool,
    dst: usize,
    val: &Value,
) -> bool {
    match val {
        Value::Int(i) => write_scalar(t, v, heap, free_heap, local_count, dst, TAG_INT, *i as u64),
        Value::Float(f) => write_scalar(
            t,
            v,
            heap,
            free_heap,
            local_count,
            dst,
            TAG_FLOAT,
            f.to_bits(),
        ),
        Value::Bool(b) => write_scalar(
            t,
            v,
            heap,
            free_heap,
            local_count,
            dst,
            TAG_BOOL,
            u64::from(*b),
        ),
        Value::Null => write_scalar(t, v, heap, free_heap, local_count, dst, TAG_NULL, 0),
        other if !other.is_reference() => write_heap(
            t,
            v,
            heap,
            free_heap,
            local_count,
            has_heap,
            dst,
            other.clone(),
        ),
        _ => false,
    }
}

/// Construct an owned `Value` from `(tag, bit)` without mutating `src`.
/// Returns `None` for `TAG_UNINIT` or a `Value::Reference`.
#[inline(always)]
fn read_owned_value(tag: u8, bit: u64, heap: &[Option<Value>]) -> Option<Value> {
    match tag {
        TAG_INT => Some(Value::Int(bit as i64)),
        TAG_FLOAT => Some(Value::Float(f64::from_bits(bit))),
        TAG_BOOL => Some(Value::Bool(bit != 0)),
        TAG_NULL => Some(Value::Null),
        TAG_HEAP => {
            let val = heap.get(bit as usize)?.as_ref()?;
            if val.is_reference() {
                None
            } else {
                Some(val.clone())
            }
        }
        _ => None,
    }
}

/// Run the Tier-2 compact VM starting from `position` inside `entry_fast`.
///
/// Kept `#[inline(never)]` so it adds zero register pressure or code size to
/// `Machine::run` / `run_simple`.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_fast(
    code: &CodeTable,
    fast_table: &[FastEntry],
    stack: &mut CallStack,
    registers: &mut Registers,
    fast_stack: &mut FastStack,
    position: Position,
    entry_fast: &FastFunction,
    entry_fast_ip: usize,
    stop: usize,
    frame_limit: usize,
    work_io: &mut usize,
    executed_io: &mut u64,
) -> FastExit {
    let entry_frame = position.frame;
    let entry_vm_base = entry_frame.base();
    let entry_depth = stack.depth();
    let entry_can_fast_return = entry_depth > stop + 1
        && entry_frame.kind() == CallKind::Function
        && entry_frame.destination().is_some()
        && !entry_frame.has_extras()
        && code.facts(entry_frame.function).local.is_some()
        && !stack.has_calls_from(entry_frame.start());

    fast_stack.reset();
    let mut cur_has_heap =
        fast_stack.load_entry_from_vm(registers, entry_vm_base, entry_fast.reg_count as usize);

    let mut cur_fn_idx = entry_frame.function;
    let mut cur_fn: &FastFunction = entry_fast;
    let mut cur_ops = &cur_fn.ops[..];
    let mut cur_f64 = &cur_fn.f64_consts[..];
    let mut cur_local_count = cur_fn.local_count;
    let mut cur_reg_count = cur_fn.reg_count;

    let mut base: usize = 0;
    let mut ip: usize = entry_fast_ip;
    let mut work: usize = *work_io;
    let mut executed: u64 = 0;

    // Hot dispatch loop. Re-slices fixed 256-slot register windows only when
    // `base` changes (on call / return).
    'frame_loop: loop {
        fast_stack.ensure_window(base);
        let (t, v): (&mut [u8; 256], &mut [u64; 256]) = {
            let t_slice = &mut fast_stack.tags[base..base + 256];
            let v_slice = &mut fast_stack.bits[base..base + 256];
            (
                t_slice.try_into().expect("256-tag window"),
                v_slice.try_into().expect("256-bit window"),
            )
        };
        let heap = &mut fast_stack.heap;
        let free_heap = &mut fast_stack.free_heap;

        loop {
            let Some(&op) = cur_ops.get(ip) else {
                // Should never happen in valid bytecode; fall back safely.
                break 'frame_loop;
            };
            executed += 1;

            match op.op {
                // =============================================================
                // FUSED LOOP LATCHES (1 op per loop round!)
                // =============================================================
                Op::IncLoopLtImm => {
                    let r = op.a as usize;
                    if t[r] == TAG_INT {
                        let next = (v[r] as i64).wrapping_add(1);
                        if next != i64::MIN {
                            v[r] = next as u64;
                            if next < i64::from(op.imm) {
                                ip = (op.b as usize) | ((op.c as usize) << 8);
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
                                        stack,
                                        registers,
                                        fast_stack,
                                        entry_frame,
                                        entry_fast.reg_count as usize,
                                        cur_fn_idx,
                                        cur_fn,
                                        base,
                                        ip,
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        executed,
                                    );
                                }
                                continue;
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::IncLoopLeImm => {
                    let r = op.a as usize;
                    if t[r] == TAG_INT {
                        let next = (v[r] as i64).wrapping_add(1);
                        if next != i64::MIN {
                            v[r] = next as u64;
                            if next <= i64::from(op.imm) {
                                ip = (op.b as usize) | ((op.c as usize) << 8);
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
                                        stack,
                                        registers,
                                        fast_stack,
                                        entry_frame,
                                        entry_fast.reg_count as usize,
                                        cur_fn_idx,
                                        cur_fn,
                                        base,
                                        ip,
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        executed,
                                    );
                                }
                                continue;
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::DecLoopGeImm => {
                    let r = op.a as usize;
                    if t[r] == TAG_INT {
                        let next = (v[r] as i64).wrapping_sub(1);
                        if next != i64::MAX {
                            v[r] = next as u64;
                            if next >= i64::from(op.imm) {
                                ip = (op.b as usize) | ((op.c as usize) << 8);
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
                                        stack,
                                        registers,
                                        fast_stack,
                                        entry_frame,
                                        entry_fast.reg_count as usize,
                                        cur_fn_idx,
                                        cur_fn,
                                        base,
                                        ip,
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        executed,
                                    );
                                }
                                continue;
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::DecLoopGtImm => {
                    let r = op.a as usize;
                    if t[r] == TAG_INT {
                        let next = (v[r] as i64).wrapping_sub(1);
                        if next != i64::MAX {
                            v[r] = next as u64;
                            if next > i64::from(op.imm) {
                                ip = (op.b as usize) | ((op.c as usize) << 8);
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
                                        stack,
                                        registers,
                                        fast_stack,
                                        entry_frame,
                                        entry_fast.reg_count as usize,
                                        cur_fn_idx,
                                        cur_fn,
                                        base,
                                        ip,
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        executed,
                                    );
                                }
                                continue;
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::IncLoopLtReg => {
                    let r = op.a as usize;
                    let lim = op.b as usize;
                    if t[r] == TAG_INT && t[lim] == TAG_INT {
                        let next = (v[r] as i64).wrapping_add(1);
                        if next != i64::MIN {
                            v[r] = next as u64;
                            if next < (v[lim] as i64) {
                                ip = op.imm as usize;
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
                                        stack,
                                        registers,
                                        fast_stack,
                                        entry_frame,
                                        entry_fast.reg_count as usize,
                                        cur_fn_idx,
                                        cur_fn,
                                        base,
                                        ip,
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        executed,
                                    );
                                }
                                continue;
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::IncLoopLeReg => {
                    let r = op.a as usize;
                    let lim = op.b as usize;
                    if t[r] == TAG_INT && t[lim] == TAG_INT {
                        let next = (v[r] as i64).wrapping_add(1);
                        if next != i64::MIN {
                            v[r] = next as u64;
                            if next <= (v[lim] as i64) {
                                ip = op.imm as usize;
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
                                        stack,
                                        registers,
                                        fast_stack,
                                        entry_frame,
                                        entry_fast.reg_count as usize,
                                        cur_fn_idx,
                                        cur_fn,
                                        base,
                                        ip,
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        executed,
                                    );
                                }
                                continue;
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::DecLoopGeReg => {
                    let r = op.a as usize;
                    let lim = op.b as usize;
                    if t[r] == TAG_INT && t[lim] == TAG_INT {
                        let next = (v[r] as i64).wrapping_sub(1);
                        if next != i64::MAX {
                            v[r] = next as u64;
                            if next >= (v[lim] as i64) {
                                ip = op.imm as usize;
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
                                        stack,
                                        registers,
                                        fast_stack,
                                        entry_frame,
                                        entry_fast.reg_count as usize,
                                        cur_fn_idx,
                                        cur_fn,
                                        base,
                                        ip,
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        executed,
                                    );
                                }
                                continue;
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::DecLoopGtReg => {
                    let r = op.a as usize;
                    let lim = op.b as usize;
                    if t[r] == TAG_INT && t[lim] == TAG_INT {
                        let next = (v[r] as i64).wrapping_sub(1);
                        if next != i64::MAX {
                            v[r] = next as u64;
                            if next > (v[lim] as i64) {
                                ip = op.imm as usize;
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
                                        stack,
                                        registers,
                                        fast_stack,
                                        entry_frame,
                                        entry_fast.reg_count as usize,
                                        cur_fn_idx,
                                        cur_fn,
                                        base,
                                        ip,
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        executed,
                                    );
                                }
                                continue;
                            }
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }

                // =============================================================
                // NUMERIC ARITHMETIC (INT + FLOAT + MIXED, EXACT OVERFLOW)
                // =============================================================
                Op::AddNum => {
                    let lt = t[op.b as usize];
                    let rt = t[op.c as usize];
                    let (out_tag, out_bit) = if lt == TAG_INT && rt == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        let b = v[op.c as usize] as i64;
                        match a.checked_add(b) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (TAG_FLOAT, ((a as f64) + (b as f64)).to_bits()),
                        }
                    } else if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) + f64::from_bits(v[op.c as usize]))
                                .to_bits(),
                        )
                    } else if lt == TAG_FLOAT && rt == TAG_INT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) + (v[op.c as usize] as i64) as f64)
                                .to_bits(),
                        )
                    } else if lt == TAG_INT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            ((v[op.b as usize] as i64) as f64 + f64::from_bits(v[op.c as usize]))
                                .to_bits(),
                        )
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        out_tag,
                        out_bit,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubNum => {
                    let lt = t[op.b as usize];
                    let rt = t[op.c as usize];
                    let (out_tag, out_bit) = if lt == TAG_INT && rt == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        let b = v[op.c as usize] as i64;
                        match a.checked_sub(b) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (TAG_FLOAT, ((a as f64) - (b as f64)).to_bits()),
                        }
                    } else if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) - f64::from_bits(v[op.c as usize]))
                                .to_bits(),
                        )
                    } else if lt == TAG_FLOAT && rt == TAG_INT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) - (v[op.c as usize] as i64) as f64)
                                .to_bits(),
                        )
                    } else if lt == TAG_INT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            ((v[op.b as usize] as i64) as f64 - f64::from_bits(v[op.c as usize]))
                                .to_bits(),
                        )
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        out_tag,
                        out_bit,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulNum => {
                    let lt = t[op.b as usize];
                    let rt = t[op.c as usize];
                    let (out_tag, out_bit) = if lt == TAG_INT && rt == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        let b = v[op.c as usize] as i64;
                        match a.checked_mul(b) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (TAG_FLOAT, ((a as f64) * (b as f64)).to_bits()),
                        }
                    } else if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) * f64::from_bits(v[op.c as usize]))
                                .to_bits(),
                        )
                    } else if lt == TAG_FLOAT && rt == TAG_INT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) * (v[op.c as usize] as i64) as f64)
                                .to_bits(),
                        )
                    } else if lt == TAG_INT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            ((v[op.b as usize] as i64) as f64 * f64::from_bits(v[op.c as usize]))
                                .to_bits(),
                        )
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        out_tag,
                        out_bit,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::DivNum => {
                    let lt = t[op.b as usize];
                    let rt = t[op.c as usize];
                    let (out_tag, out_bit) = if lt == TAG_INT && rt == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        let b = v[op.c as usize] as i64;
                        if b == 0 {
                            break 'frame_loop;
                        }
                        if a == i64::MIN && b == -1 {
                            (TAG_FLOAT, ((a as f64) / (b as f64)).to_bits())
                        } else if a % b == 0 {
                            (TAG_INT, (a / b) as u64)
                        } else {
                            (TAG_FLOAT, ((a as f64) / (b as f64)).to_bits())
                        }
                    } else if (lt == TAG_INT || lt == TAG_FLOAT)
                        && (rt == TAG_INT || rt == TAG_FLOAT)
                    {
                        let a = if lt == TAG_FLOAT {
                            f64::from_bits(v[op.b as usize])
                        } else {
                            (v[op.b as usize] as i64) as f64
                        };
                        let b = if rt == TAG_FLOAT {
                            f64::from_bits(v[op.c as usize])
                        } else {
                            (v[op.c as usize] as i64) as f64
                        };
                        if b == 0.0 {
                            break 'frame_loop;
                        }
                        (TAG_FLOAT, (a / b).to_bits())
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        out_tag,
                        out_bit,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ModInt => {
                    if t[op.b as usize] == TAG_INT && t[op.c as usize] == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        let b = v[op.c as usize] as i64;
                        if b == 0 {
                            break 'frame_loop;
                        }
                        let res = if a == i64::MIN && b == -1 { 0 } else { a % b };
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::BitAnd => {
                    if t[op.b as usize] == TAG_INT && t[op.c as usize] == TAG_INT {
                        let res = (v[op.b as usize] as i64) & (v[op.c as usize] as i64);
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::BitOr => {
                    if t[op.b as usize] == TAG_INT && t[op.c as usize] == TAG_INT {
                        let res = (v[op.b as usize] as i64) | (v[op.c as usize] as i64);
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::BitXor => {
                    if t[op.b as usize] == TAG_INT && t[op.c as usize] == TAG_INT {
                        let res = (v[op.b as usize] as i64) ^ (v[op.c as usize] as i64);
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::Shl => {
                    if t[op.b as usize] == TAG_INT && t[op.c as usize] == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        let b = v[op.c as usize] as i64;
                        if b < 0 {
                            break 'frame_loop;
                        }
                        let res = if b >= 64 { 0 } else { a.wrapping_shl(b as u32) };
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::Shr => {
                    if t[op.b as usize] == TAG_INT && t[op.c as usize] == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        let b = v[op.c as usize] as i64;
                        if b < 0 {
                            break 'frame_loop;
                        }
                        let res = if b >= 64 {
                            a >> 63
                        } else {
                            a.wrapping_shr(b as u32)
                        };
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }

                // =============================================================
                // IMMEDIATE ARITHMETIC
                // =============================================================
                Op::AddImm => {
                    let lt = t[op.b as usize];
                    let imm = i64::from(op.imm);
                    let (out_tag, out_bit) = if lt == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        match a.checked_add(imm) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (TAG_FLOAT, ((a as f64) + (imm as f64)).to_bits()),
                        }
                    } else if lt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) + (imm as f64)).to_bits(),
                        )
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        out_tag,
                        out_bit,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubImm => {
                    let lt = t[op.b as usize];
                    let imm = i64::from(op.imm);
                    let (out_tag, out_bit) = if lt == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        match a.checked_sub(imm) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (TAG_FLOAT, ((a as f64) - (imm as f64)).to_bits()),
                        }
                    } else if lt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) - (imm as f64)).to_bits(),
                        )
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        out_tag,
                        out_bit,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulImm => {
                    let lt = t[op.b as usize];
                    let imm = i64::from(op.imm);
                    let (out_tag, out_bit) = if lt == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        match a.checked_mul(imm) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (TAG_FLOAT, ((a as f64) * (imm as f64)).to_bits()),
                        }
                    } else if lt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(v[op.b as usize]) * (imm as f64)).to_bits(),
                        )
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        out_tag,
                        out_bit,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ModImm => {
                    if t[op.b as usize] == TAG_INT && op.imm != 0 {
                        let a = v[op.b as usize] as i64;
                        let b = i64::from(op.imm);
                        let res = if a == i64::MIN && b == -1 { 0 } else { a % b };
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::BitAndImm => {
                    if t[op.b as usize] == TAG_INT {
                        let res = (v[op.b as usize] as i64) & i64::from(op.imm);
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::BitOrImm => {
                    if t[op.b as usize] == TAG_INT {
                        let res = (v[op.b as usize] as i64) | i64::from(op.imm);
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::BitXorImm => {
                    if t[op.b as usize] == TAG_INT {
                        let res = (v[op.b as usize] as i64) ^ i64::from(op.imm);
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::ShlImm => {
                    if t[op.b as usize] == TAG_INT && op.imm >= 0 {
                        let a = v[op.b as usize] as i64;
                        let res = if op.imm >= 64 {
                            0
                        } else {
                            a.wrapping_shl(op.imm as u32)
                        };
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::ShrImm => {
                    if t[op.b as usize] == TAG_INT && op.imm >= 0 {
                        let a = v[op.b as usize] as i64;
                        let res = if op.imm >= 64 {
                            a >> 63
                        } else {
                            a.wrapping_shr(op.imm as u32)
                        };
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }

                // =============================================================
                // COMPOUND LOCAL UPDATES
                // =============================================================
                Op::AddAssign => {
                    let loc = op.a as usize;
                    let src = op.b as usize;
                    let lt = t[loc];
                    let rt = t[src];
                    if lt == TAG_INT && rt == TAG_INT {
                        let a = v[loc] as i64;
                        let b = v[src] as i64;
                        match a.checked_add(b) {
                            Some(s) => v[loc] = s as u64,
                            None => {
                                t[loc] = TAG_FLOAT;
                                v[loc] = ((a as f64) + (b as f64)).to_bits();
                            }
                        }
                        ip += 1;
                    } else if (lt == TAG_INT || lt == TAG_FLOAT)
                        && (rt == TAG_INT || rt == TAG_FLOAT)
                    {
                        let a = if lt == TAG_FLOAT {
                            f64::from_bits(v[loc])
                        } else {
                            (v[loc] as i64) as f64
                        };
                        let b = if rt == TAG_FLOAT {
                            f64::from_bits(v[src])
                        } else {
                            (v[src] as i64) as f64
                        };
                        t[loc] = TAG_FLOAT;
                        v[loc] = (a + b).to_bits();
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::SubAssign => {
                    let loc = op.a as usize;
                    let src = op.b as usize;
                    let lt = t[loc];
                    let rt = t[src];
                    if lt == TAG_INT && rt == TAG_INT {
                        let a = v[loc] as i64;
                        let b = v[src] as i64;
                        match a.checked_sub(b) {
                            Some(s) => v[loc] = s as u64,
                            None => {
                                t[loc] = TAG_FLOAT;
                                v[loc] = ((a as f64) - (b as f64)).to_bits();
                            }
                        }
                        ip += 1;
                    } else if (lt == TAG_INT || lt == TAG_FLOAT)
                        && (rt == TAG_INT || rt == TAG_FLOAT)
                    {
                        let a = if lt == TAG_FLOAT {
                            f64::from_bits(v[loc])
                        } else {
                            (v[loc] as i64) as f64
                        };
                        let b = if rt == TAG_FLOAT {
                            f64::from_bits(v[src])
                        } else {
                            (v[src] as i64) as f64
                        };
                        t[loc] = TAG_FLOAT;
                        v[loc] = (a - b).to_bits();
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::MulAssign => {
                    let loc = op.a as usize;
                    let src = op.b as usize;
                    let lt = t[loc];
                    let rt = t[src];
                    if lt == TAG_INT && rt == TAG_INT {
                        let a = v[loc] as i64;
                        let b = v[src] as i64;
                        match a.checked_mul(b) {
                            Some(s) => v[loc] = s as u64,
                            None => {
                                t[loc] = TAG_FLOAT;
                                v[loc] = ((a as f64) * (b as f64)).to_bits();
                            }
                        }
                        ip += 1;
                    } else if (lt == TAG_INT || lt == TAG_FLOAT)
                        && (rt == TAG_INT || rt == TAG_FLOAT)
                    {
                        let a = if lt == TAG_FLOAT {
                            f64::from_bits(v[loc])
                        } else {
                            (v[loc] as i64) as f64
                        };
                        let b = if rt == TAG_FLOAT {
                            f64::from_bits(v[src])
                        } else {
                            (v[src] as i64) as f64
                        };
                        t[loc] = TAG_FLOAT;
                        v[loc] = (a * b).to_bits();
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::StrAppend => {
                    let loc = op.a as usize;
                    let src = op.b as usize;
                    if t[loc] == TAG_HEAP && t[src] == TAG_HEAP {
                        let loc_idx = v[loc] as usize;
                        let src_idx = v[src] as usize;
                        if loc_idx == src_idx {
                            break 'frame_loop;
                        }
                        let interned: Option<&'static [u8]> = match heap.get(src_idx) {
                            Some(Some(Value::InternedStr(s))) => Some(s.as_bytes()),
                            Some(Some(Value::String(_))) => None,
                            _ => break 'frame_loop,
                        };
                        if let Some(bytes) = interned {
                            match heap.get_mut(loc_idx) {
                                Some(Some(slot @ Value::InternedStr(_))) => {
                                    let Value::InternedStr(existing) = *slot else {
                                        unreachable!()
                                    };
                                    let mut owned = existing.to_php_string();
                                    owned.push_bytes(bytes);
                                    *slot = Value::String(owned);
                                }
                                Some(Some(Value::String(existing))) => {
                                    existing.push_bytes(bytes);
                                }
                                _ => break 'frame_loop,
                            }
                            ip += 1;
                        } else {
                            let Some(Some(Value::String(src_s))) = heap.get(src_idx) else {
                                break 'frame_loop;
                            };
                            let src_cloned = src_s.clone();
                            match heap.get_mut(loc_idx) {
                                Some(Some(slot @ Value::InternedStr(_))) => {
                                    let Value::InternedStr(existing) = *slot else {
                                        unreachable!()
                                    };
                                    let mut owned = existing.to_php_string();
                                    owned.push_bytes(src_cloned.as_bytes());
                                    *slot = Value::String(owned);
                                }
                                Some(Some(Value::String(existing))) => {
                                    existing.push_bytes(src_cloned.as_bytes());
                                }
                                _ => break 'frame_loop,
                            }
                            ip += 1;
                        }
                    } else {
                        break 'frame_loop;
                    }
                }

                // =============================================================
                // CONSTANTS, MOVES, CLEARS
                // =============================================================
                Op::LoadInt => {
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_INT,
                        i64::from(op.imm) as u64,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadF64 => {
                    let bits = cur_f64[op.imm as usize].to_bits();
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_FLOAT,
                        bits,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadBool => {
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_BOOL,
                        u64::from(op.b),
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadNull => {
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_NULL,
                        0,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadConstVal => {
                    let Some(val) = code.cache(cur_fn_idx).constants.get(op.imm as usize) else {
                        break 'frame_loop;
                    };
                    if !store_borrowed_value(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        &mut cur_has_heap,
                        op.a as usize,
                        val,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::NewArray => {
                    let array = if op.imm > 0 {
                        PhpArray::with_capacity(op.imm as usize)
                    } else {
                        PhpArray::new()
                    };
                    if !write_heap(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        &mut cur_has_heap,
                        op.a as usize,
                        Value::Array(Rc::new(array)),
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Move | Op::Assign => {
                    let dst = op.a as usize;
                    let src = op.b as usize;
                    let st = t[src];
                    if st == TAG_UNINIT {
                        break 'frame_loop;
                    }
                    if st != TAG_HEAP {
                        if !write_scalar(t, v, heap, free_heap, cur_local_count, dst, st, v[src]) {
                            break 'frame_loop;
                        }
                    } else {
                        let Some(val) = read_owned_value(st, v[src], heap) else {
                            break 'frame_loop;
                        };
                        if !write_heap(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            &mut cur_has_heap,
                            dst,
                            val,
                        ) {
                            break 'frame_loop;
                        }
                    }
                    ip += 1;
                }
                Op::AssignTake => {
                    let dst = op.a as usize;
                    let src = op.b as usize;
                    let st = t[src];
                    if st == TAG_UNINIT {
                        break 'frame_loop;
                    }
                    if st != TAG_HEAP {
                        if !write_scalar(t, v, heap, free_heap, cur_local_count, dst, st, v[src]) {
                            break 'frame_loop;
                        }
                        t[src] = TAG_UNINIT;
                    } else {
                        let src_idx = v[src] as usize;
                        if heap
                            .get(src_idx)
                            .and_then(Option::as_ref)
                            .is_some_and(Value::is_reference)
                        {
                            break 'frame_loop;
                        }
                        if t[dst] == TAG_HEAP
                            && dst < cur_local_count as usize
                            && heap
                                .get(v[dst] as usize)
                                .and_then(Option::as_ref)
                                .is_some_and(Value::is_reference)
                        {
                            break 'frame_loop;
                        }
                        if dst != src {
                            if t[dst] == TAG_HEAP {
                                let old_idx = v[dst] as u32;
                                if let Some(slot) = heap.get_mut(old_idx as usize) {
                                    *slot = None;
                                    free_heap.push(old_idx);
                                }
                            }
                            t[dst] = TAG_HEAP;
                            v[dst] = v[src];
                            t[src] = TAG_UNINIT;
                            cur_has_heap = true;
                        }
                    }
                    ip += 1;
                }
                Op::Clear => {
                    let r = op.a as usize;
                    if t[r] == TAG_HEAP {
                        let idx = v[r] as u32;
                        if let Some(slot) = heap.get_mut(idx as usize) {
                            *slot = None;
                            free_heap.push(idx);
                        }
                    }
                    t[r] = TAG_UNINIT;
                    ip += 1;
                }

                // =============================================================
                // UNARY, CASTS, BUILTINS, INC/DEC
                // =============================================================
                Op::NegNum => {
                    let st = t[op.b as usize];
                    let (out_tag, out_bit) = if st == TAG_INT {
                        let a = v[op.b as usize] as i64;
                        match a.checked_neg() {
                            Some(n) => (TAG_INT, n as u64),
                            None => (TAG_FLOAT, (-(a as f64)).to_bits()),
                        }
                    } else if st == TAG_FLOAT {
                        (TAG_FLOAT, (-f64::from_bits(v[op.b as usize])).to_bits())
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        out_tag,
                        out_bit,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::NotBool => {
                    let Some(truthy) = scalar_truthy(t[op.b as usize], v[op.b as usize]) else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_BOOL,
                        u64::from(!truthy),
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitNotInt => {
                    if t[op.b as usize] == TAG_INT {
                        let res = !(v[op.b as usize] as i64);
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            res as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::CastInt => {
                    let st = t[op.b as usize];
                    let out: i64 = match st {
                        TAG_INT => v[op.b as usize] as i64,
                        TAG_FLOAT => {
                            let f = f64::from_bits(v[op.b as usize]);
                            if f.is_finite()
                                && (-9223372036854775808.0..9223372036854775808.0).contains(&f)
                            {
                                f as i64
                            } else {
                                break 'frame_loop;
                            }
                        }
                        TAG_BOOL => i64::from(v[op.b as usize] != 0),
                        TAG_NULL => 0,
                        _ => break 'frame_loop,
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_INT,
                        out as u64,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::CastFloat => {
                    let st = t[op.b as usize];
                    let out: f64 = match st {
                        TAG_FLOAT => f64::from_bits(v[op.b as usize]),
                        TAG_INT => (v[op.b as usize] as i64) as f64,
                        TAG_BOOL => {
                            if v[op.b as usize] != 0 {
                                1.0
                            } else {
                                0.0
                            }
                        }
                        TAG_NULL => 0.0,
                        _ => break 'frame_loop,
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_FLOAT,
                        out.to_bits(),
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::CastBool => {
                    let Some(truthy) = scalar_truthy(t[op.b as usize], v[op.b as usize]) else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_BOOL,
                        u64::from(truthy),
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::StrLen => {
                    if t[op.b as usize] == TAG_HEAP {
                        let len = match heap.get(v[op.b as usize] as usize) {
                            Some(Some(Value::InternedStr(s))) => s.len() as i64,
                            Some(Some(Value::String(s))) => s.len() as i64,
                            _ => break 'frame_loop,
                        };
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            len as u64,
                        ) {
                            break 'frame_loop;
                        }
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::IncLocalFast => {
                    let r = op.a as usize;
                    if t[r] == TAG_INT {
                        let next = (v[r] as i64).wrapping_add(1);
                        if next != i64::MIN {
                            v[r] = next as u64;
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::DecLocalFast => {
                    let r = op.a as usize;
                    if t[r] == TAG_INT {
                        let next = (v[r] as i64).wrapping_sub(1);
                        if next != i64::MAX {
                            v[r] = next as u64;
                            ip += 1;
                            continue;
                        }
                    }
                    break 'frame_loop;
                }
                Op::IncDecLocal => {
                    let dst = op.a as usize;
                    let loc = op.b as usize;
                    if t[loc] == TAG_INT {
                        let old = v[loc] as i64;
                        let new = if op.c < 2 {
                            match old.checked_add(1) {
                                Some(n) => n,
                                None => break 'frame_loop,
                            }
                        } else {
                            match old.checked_sub(1) {
                                Some(n) => n,
                                None => break 'frame_loop,
                            }
                        };
                        let ret = if op.c & 1 == 1 { old } else { new };
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            dst,
                            TAG_INT,
                            ret as u64,
                        ) {
                            break 'frame_loop;
                        }
                        v[loc] = new as u64;
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }

                // =============================================================
                // COMPARISONS & BRANCHES
                // =============================================================
                Op::CmpNum => {
                    let Some(res) = compare_nums(
                        op.imm as u8,
                        t[op.b as usize],
                        v[op.b as usize],
                        t[op.c as usize],
                        v[op.c as usize],
                    ) else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        heap,
                        free_heap,
                        cur_local_count,
                        op.a as usize,
                        TAG_BOOL,
                        u64::from(res),
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Jump => {
                    let target = op.imm as usize;
                    if target <= ip {
                        work += 1;
                        if work >= YIELD_AFTER_STEPS {
                            work = 0;
                            return materialize_deopt(
                                stack,
                                registers,
                                fast_stack,
                                entry_frame,
                                entry_fast.reg_count as usize,
                                cur_fn_idx,
                                cur_fn,
                                base,
                                target,
                                true,
                                work_io,
                                executed_io,
                                work,
                                executed,
                            );
                        }
                    }
                    ip = target;
                }
                Op::JumpIf => {
                    let Some(truthy) = scalar_truthy(t[op.a as usize], v[op.a as usize]) else {
                        break 'frame_loop;
                    };
                    if truthy {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    stack,
                                    registers,
                                    fast_stack,
                                    entry_frame,
                                    entry_fast.reg_count as usize,
                                    cur_fn_idx,
                                    cur_fn,
                                    base,
                                    target,
                                    true,
                                    work_io,
                                    executed_io,
                                    work,
                                    executed,
                                );
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }
                Op::JumpIfNot => {
                    let Some(truthy) = scalar_truthy(t[op.a as usize], v[op.a as usize]) else {
                        break 'frame_loop;
                    };
                    if !truthy {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    stack,
                                    registers,
                                    fast_stack,
                                    entry_frame,
                                    entry_fast.reg_count as usize,
                                    cur_fn_idx,
                                    cur_fn,
                                    base,
                                    target,
                                    true,
                                    work_io,
                                    executed_io,
                                    work,
                                    executed,
                                );
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }
                Op::JumpIfNull => {
                    let tag = t[op.a as usize];
                    if tag == TAG_UNINIT {
                        break 'frame_loop;
                    }
                    let is_null = if tag == TAG_NULL {
                        true
                    } else if tag == TAG_HEAP {
                        match heap.get(v[op.a as usize] as usize) {
                            Some(Some(val)) if !val.is_reference() => val.is_null(),
                            _ => break 'frame_loop,
                        }
                    } else {
                        false
                    };
                    if is_null {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    stack,
                                    registers,
                                    fast_stack,
                                    entry_frame,
                                    entry_fast.reg_count as usize,
                                    cur_fn_idx,
                                    cur_fn,
                                    base,
                                    target,
                                    true,
                                    work_io,
                                    executed_io,
                                    work,
                                    executed,
                                );
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }
                Op::JumpIfNotNull => {
                    let tag = t[op.a as usize];
                    if tag == TAG_UNINIT {
                        break 'frame_loop;
                    }
                    let is_null = if tag == TAG_NULL {
                        true
                    } else if tag == TAG_HEAP {
                        match heap.get(v[op.a as usize] as usize) {
                            Some(Some(val)) if !val.is_reference() => val.is_null(),
                            _ => break 'frame_loop,
                        }
                    } else {
                        false
                    };
                    if !is_null {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    stack,
                                    registers,
                                    fast_stack,
                                    entry_frame,
                                    entry_fast.reg_count as usize,
                                    cur_fn_idx,
                                    cur_fn,
                                    base,
                                    target,
                                    true,
                                    work_io,
                                    executed_io,
                                    work,
                                    executed,
                                );
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }
                Op::BranchCmp => {
                    let cmp = op.a & 0x0f;
                    let when = (op.a & 0x10) != 0;
                    let Some(res) = compare_nums(
                        cmp,
                        t[op.b as usize],
                        v[op.b as usize],
                        t[op.c as usize],
                        v[op.c as usize],
                    ) else {
                        break 'frame_loop;
                    };
                    if res == when {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    stack,
                                    registers,
                                    fast_stack,
                                    entry_frame,
                                    entry_fast.reg_count as usize,
                                    cur_fn_idx,
                                    cur_fn,
                                    base,
                                    target,
                                    true,
                                    work_io,
                                    executed_io,
                                    work,
                                    executed,
                                );
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }
                Op::BranchLtImm
                | Op::BranchLeImm
                | Op::BranchGtImm
                | Op::BranchGeImm
                | Op::BranchEqImm
                | Op::BranchNeImm
                | Op::BranchIdentImm
                | Op::BranchNotIdentImm => {
                    let lt = t[op.a as usize];
                    let imm = i64::from(op.imm);
                    let res = if lt == TAG_INT {
                        let a = v[op.a as usize] as i64;
                        match op.op {
                            Op::BranchLtImm => a < imm,
                            Op::BranchLeImm => a <= imm,
                            Op::BranchGtImm => a > imm,
                            Op::BranchGeImm => a >= imm,
                            Op::BranchEqImm | Op::BranchIdentImm => a == imm,
                            Op::BranchNeImm | Op::BranchNotIdentImm => a != imm,
                            _ => unreachable!(),
                        }
                    } else if lt == TAG_FLOAT {
                        let a = f64::from_bits(v[op.a as usize]);
                        let b = imm as f64;
                        match op.op {
                            Op::BranchLtImm => a < b,
                            Op::BranchLeImm => a <= b,
                            Op::BranchGtImm => a > b,
                            Op::BranchGeImm => a >= b,
                            Op::BranchEqImm => a == b,
                            Op::BranchNeImm => a != b,
                            Op::BranchIdentImm => false,
                            Op::BranchNotIdentImm => true,
                            _ => unreachable!(),
                        }
                    } else if matches!(op.op, Op::BranchIdentImm | Op::BranchNotIdentImm)
                        && lt != TAG_UNINIT
                        && (lt != TAG_HEAP
                            || heap
                                .get(v[op.a as usize] as usize)
                                .and_then(Option::as_ref)
                                .is_some_and(|val| !val.is_reference()))
                    {
                        matches!(op.op, Op::BranchNotIdentImm)
                    } else {
                        break 'frame_loop;
                    };
                    let when = (op.c & 0x80) != 0;
                    if res == when {
                        let target = (op.b as usize) | (((op.c & 0x7f) as usize) << 8);
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    stack,
                                    registers,
                                    fast_stack,
                                    entry_frame,
                                    entry_fast.reg_count as usize,
                                    cur_fn_idx,
                                    cur_fn,
                                    base,
                                    target,
                                    true,
                                    work_io,
                                    executed_io,
                                    work,
                                    executed,
                                );
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }

                // =============================================================
                // IN-LOOP ARRAY FAST PATHS (1D, 2D, WRITE, ADD-ASSIGN, PUSH, UNSET)
                // =============================================================
                Op::ArrGetInt => {
                    let arr_r = op.b as usize;
                    let idx_r = op.c as usize;
                    if t[arr_r] != TAG_HEAP || t[idx_r] != TAG_INT {
                        break 'frame_loop;
                    }
                    let key = v[idx_r] as i64;
                    let quiet = op.imm != 0;
                    enum Found {
                        Int(i64),
                        Float(u64),
                        Bool(u64),
                        Null,
                        Heap(Value),
                        Bail,
                    }
                    let found = match heap.get(v[arr_r] as usize) {
                        Some(Some(Value::Array(rc))) => match rc.get_int(key) {
                            Some(Value::Int(i)) => Found::Int(*i),
                            Some(Value::Float(f)) => Found::Float(f.to_bits()),
                            Some(Value::Bool(b)) => Found::Bool(u64::from(*b)),
                            Some(Value::Null) => Found::Null,
                            Some(other) if !other.is_reference() => Found::Heap(other.clone()),
                            None if quiet => Found::Null,
                            _ => Found::Bail,
                        },
                        Some(Some(Value::Reference(cell))) => {
                            if let Ok(guard) = cell.try_borrow() {
                                match &*guard {
                                    Value::Array(rc) => match rc.get_int(key) {
                                        Some(Value::Int(i)) => Found::Int(*i),
                                        Some(Value::Float(f)) => Found::Float(f.to_bits()),
                                        Some(Value::Bool(b)) => Found::Bool(u64::from(*b)),
                                        Some(Value::Null) => Found::Null,
                                        Some(other) if !other.is_reference() => {
                                            Found::Heap(other.clone())
                                        }
                                        None if quiet => Found::Null,
                                        _ => Found::Bail,
                                    },
                                    _ => Found::Bail,
                                }
                            } else {
                                Found::Bail
                            }
                        }
                        _ => Found::Bail,
                    };
                    let ok = match found {
                        Found::Int(i) => write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            i as u64,
                        ),
                        Found::Float(bits) => write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_FLOAT,
                            bits,
                        ),
                        Found::Bool(b) => write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_BOOL,
                            b,
                        ),
                        Found::Null => write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_NULL,
                            0,
                        ),
                        Found::Heap(val) => write_heap(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            &mut cur_has_heap,
                            op.a as usize,
                            val,
                        ),
                        Found::Bail => false,
                    };
                    if !ok {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Arr2DGetInt => {
                    let arr_r = op.b as usize;
                    let idx1_r = op.c as usize;
                    let idx2_r = (op.imm & 0xff) as usize;
                    if t[arr_r] != TAG_HEAP || t[idx1_r] != TAG_INT || t[idx2_r] != TAG_INT {
                        break 'frame_loop;
                    }
                    let k1 = v[idx1_r] as i64;
                    let k2 = v[idx2_r] as i64;
                    let quiet = (op.imm >> 8) != 0;
                    enum Found2 {
                        Int(i64),
                        Float(u64),
                        Bool(u64),
                        Null,
                        Heap(Value),
                        Bail,
                    }
                    let found = match heap.get(v[arr_r] as usize) {
                        Some(Some(Value::Array(rc1))) => match rc1.get_int(k1) {
                            Some(Value::Array(rc2)) => match rc2.get_int(k2) {
                                Some(Value::Int(i)) => Found2::Int(*i),
                                Some(Value::Float(f)) => Found2::Float(f.to_bits()),
                                Some(Value::Bool(b)) => Found2::Bool(u64::from(*b)),
                                Some(Value::Null) => Found2::Null,
                                Some(other) if !other.is_reference() => Found2::Heap(other.clone()),
                                None if quiet => Found2::Null,
                                _ => Found2::Bail,
                            },
                            None if quiet => Found2::Null,
                            _ => Found2::Bail,
                        },
                        _ => Found2::Bail,
                    };
                    let ok = match found {
                        Found2::Int(i) => write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_INT,
                            i as u64,
                        ),
                        Found2::Float(bits) => write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_FLOAT,
                            bits,
                        ),
                        Found2::Bool(b) => write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_BOOL,
                            b,
                        ),
                        Found2::Null => write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            op.a as usize,
                            TAG_NULL,
                            0,
                        ),
                        Found2::Heap(val) => write_heap(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            &mut cur_has_heap,
                            op.a as usize,
                            val,
                        ),
                        Found2::Bail => false,
                    };
                    if !ok {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrSetInt => {
                    let arr_r = op.a as usize;
                    let idx_r = op.b as usize;
                    let src_r = op.c as usize;
                    if t[arr_r] != TAG_HEAP || t[idx_r] != TAG_INT {
                        break 'frame_loop;
                    }
                    let key = v[idx_r] as i64;
                    let Some(val) = read_owned_value(t[src_r], v[src_r], heap) else {
                        break 'frame_loop;
                    };
                    let ok = match heap.get_mut(v[arr_r] as usize) {
                        Some(Some(Value::Array(rc))) => {
                            Rc::make_mut(rc).insert_int_if_not_ref(key, val).is_ok()
                        }
                        Some(Some(Value::Reference(cell))) => {
                            if let Ok(mut guard) = cell.try_borrow_mut() {
                                match &mut *guard {
                                    Value::Array(rc) => {
                                        Rc::make_mut(rc).insert_int_if_not_ref(key, val).is_ok()
                                    }
                                    _ => false,
                                }
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };
                    if !ok {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrAddAssignInt => {
                    let arr_r = op.a as usize;
                    let idx_r = op.b as usize;
                    let src_r = op.c as usize;
                    if t[arr_r] != TAG_HEAP || t[idx_r] != TAG_INT {
                        break 'frame_loop;
                    }
                    let st = t[src_r];
                    if st != TAG_INT && st != TAG_FLOAT {
                        break 'frame_loop;
                    }
                    let key = v[idx_r] as i64;
                    let rhs_bit = v[src_r];
                    let ok = match heap.get_mut(v[arr_r] as usize) {
                        Some(Some(Value::Array(rc))) => {
                            let target = Rc::make_mut(rc);
                            if let Some(slot) = target.get_mut_int(key) {
                                match (slot, st) {
                                    (Value::Int(lhs), TAG_INT) => {
                                        let rhs = rhs_bit as i64;
                                        match lhs.checked_add(rhs) {
                                            Some(sum) => {
                                                *lhs = sum;
                                                true
                                            }
                                            None => {
                                                *target.get_mut_int(key).unwrap() =
                                                    Value::Float((*lhs as f64) + (rhs as f64));
                                                true
                                            }
                                        }
                                    }
                                    (slot @ Value::Float(_), TAG_FLOAT) => {
                                        let Value::Float(lhs) = *slot else {
                                            unreachable!()
                                        };
                                        *slot = Value::Float(lhs + f64::from_bits(rhs_bit));
                                        true
                                    }
                                    (slot @ Value::Float(_), TAG_INT) => {
                                        let Value::Float(lhs) = *slot else {
                                            unreachable!()
                                        };
                                        *slot = Value::Float(lhs + (rhs_bit as i64) as f64);
                                        true
                                    }
                                    (slot @ Value::Int(_), TAG_FLOAT) => {
                                        let Value::Int(lhs) = *slot else {
                                            unreachable!()
                                        };
                                        *slot =
                                            Value::Float((lhs as f64) + f64::from_bits(rhs_bit));
                                        true
                                    }
                                    _ => false,
                                }
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };
                    if !ok {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrPush => {
                    let arr_r = op.a as usize;
                    let src_r = op.b as usize;
                    if t[arr_r] != TAG_HEAP {
                        break 'frame_loop;
                    }
                    let Some(val) = read_owned_value(t[src_r], v[src_r], heap) else {
                        break 'frame_loop;
                    };
                    let ok = match heap.get_mut(v[arr_r] as usize) {
                        Some(Some(Value::Array(rc))) => Rc::make_mut(rc).push(val).is_ok(),
                        _ => false,
                    };
                    if !ok {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrUnsetInt => {
                    let arr_r = op.a as usize;
                    let idx_r = op.b as usize;
                    if t[arr_r] != TAG_HEAP || t[idx_r] != TAG_INT {
                        break 'frame_loop;
                    }
                    let key = v[idx_r] as i64;
                    let ok = match heap.get_mut(v[arr_r] as usize) {
                        Some(Some(Value::Array(rc))) => {
                            let _ = Rc::make_mut(rc).remove_int(key);
                            true
                        }
                        Some(Some(Value::Reference(cell))) => {
                            if let Ok(mut guard) = cell.try_borrow_mut() {
                                match &mut *guard {
                                    Value::Array(rc) => {
                                        let _ = Rc::make_mut(rc).remove_int(key);
                                        true
                                    }
                                    _ => false,
                                }
                            } else {
                                false
                            }
                        }
                        _ => false,
                    };
                    if !ok {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // IN-LOOP DIRECT CALLS & RETURNS (ZERO MACHINE ROUND-TRIPS!)
                // =============================================================
                Op::CallFast => {
                    let callee_idx = FunctionIndex(op.imm as u32);
                    let count = op.c as usize;
                    let args_base = op.b as usize;
                    let dst = op.a;

                    // Self-recursion skips the `fast_table` lookup completely.
                    let callee: &FastFunction = if callee_idx == cur_fn_idx {
                        cur_fn
                    } else {
                        match fast_table.get(callee_idx.index()) {
                            Some(FastEntry::Compiled(func)) => func.as_ref(),
                            _ => break 'frame_loop,
                        }
                    };

                    if !callee.is_plain
                        || !callee.takes_values_only
                        || count != callee.param_count as usize
                        || entry_depth + fast_stack.frames.len() >= frame_limit
                    {
                        break 'frame_loop;
                    }

                    // Validate all argument registers BEFORE mutating anything.
                    let mut typed = callee.typed_params;
                    let mut args_valid = true;
                    let mut callee_has_heap = false;
                    for i in 0..count {
                        let at = t[args_base + i];
                        if at == TAG_UNINIT {
                            args_valid = false;
                            break;
                        }
                        if at == TAG_HEAP {
                            let is_ref = heap
                                .get(v[args_base + i] as usize)
                                .and_then(Option::as_ref)
                                .is_none_or(Value::is_reference);
                            if is_ref {
                                args_valid = false;
                                break;
                            }
                            callee_has_heap = true;
                        }
                        if (typed & 1) != 0 {
                            let mask = callee.param_masks[i];
                            if !tag_type_mask(at, v[args_base + i], heap)
                                .is_some_and(|own| mask.intersects(own))
                            {
                                args_valid = false;
                                break;
                            }
                        }
                        typed >>= 1;
                    }
                    if !args_valid {
                        break 'frame_loop;
                    }

                    // Empty-body `[ReturnNull]` shortcut inside Tier 2!
                    if callee.is_empty_return_null
                        && callee
                            .return_mask
                            .is_none_or(|m| return_mask_fits(m, TAG_NULL, 0, heap))
                    {
                        for i in 0..count {
                            if t[args_base + i] == TAG_HEAP {
                                let idx = v[args_base + i] as u32;
                                if let Some(slot) = heap.get_mut(idx as usize) {
                                    *slot = None;
                                    free_heap.push(idx);
                                }
                            }
                            t[args_base + i] = TAG_UNINIT;
                        }
                        if !write_scalar(
                            t,
                            v,
                            heap,
                            free_heap,
                            cur_local_count,
                            dst as usize,
                            TAG_NULL,
                            0,
                        ) {
                            break 'frame_loop;
                        }
                        work += 1;
                        if work >= YIELD_AFTER_STEPS {
                            work = 0;
                            return materialize_deopt(
                                stack,
                                registers,
                                fast_stack,
                                entry_frame,
                                entry_fast.reg_count as usize,
                                cur_fn_idx,
                                cur_fn,
                                base,
                                ip + 1,
                                true,
                                work_io,
                                executed_io,
                                work,
                                executed,
                            );
                        }
                        ip += 1;
                        continue;
                    }

                    // Push caller frame & move arguments into callee's register window.
                    let next_fast_ip = (ip + 1) as u16;
                    let return_bc_ip = cur_fn.bc_ip[ip] + 1;
                    let new_base = base + cur_reg_count as usize;

                    // Copy/move args out before re-slicing `fast_stack.tags/bits`.
                    let mut arg_tags = [TAG_UNINIT; 16];
                    let mut arg_bits = [0u64; 16];
                    for i in 0..count {
                        arg_tags[i] = t[args_base + i];
                        arg_bits[i] = v[args_base + i];
                        t[args_base + i] = TAG_UNINIT;
                    }

                    fast_stack.frames.push(FastFrame {
                        function: cur_fn_idx,
                        base: base as u32,
                        reg_count: cur_reg_count,
                        has_heap: cur_has_heap,
                        return_dst: dst,
                        return_fast_ip: next_fast_ip,
                        return_bc_ip,
                    });

                    fast_stack.ensure_window(new_base);
                    fast_stack.tags[new_base..new_base + count].copy_from_slice(&arg_tags[..count]);
                    fast_stack.bits[new_base..new_base + count].copy_from_slice(&arg_bits[..count]);
                    let callee_regs = callee.reg_count as usize;
                    fast_stack.tags[new_base + count..new_base + callee_regs].fill(TAG_UNINIT);

                    cur_fn_idx = callee_idx;
                    cur_fn = callee;
                    cur_ops = &callee.ops[..];
                    cur_f64 = &callee.f64_consts[..];
                    cur_local_count = callee.local_count;
                    cur_reg_count = callee.reg_count;
                    cur_has_heap = callee_has_heap;
                    base = new_base;
                    ip = 0;

                    work += 1;
                    if work >= YIELD_AFTER_STEPS {
                        work = 0;
                        return materialize_deopt(
                            stack,
                            registers,
                            fast_stack,
                            entry_frame,
                            entry_fast.reg_count as usize,
                            cur_fn_idx,
                            cur_fn,
                            base,
                            0,
                            true,
                            work_io,
                            executed_io,
                            work,
                            executed,
                        );
                    }
                    continue 'frame_loop;
                }

                Op::RetIntImm | Op::ReturnReg | Op::ReturnNull => {
                    let (ret_tag, ret_bit) = match op.op {
                        Op::RetIntImm => (TAG_INT, i64::from(op.imm) as u64),
                        Op::ReturnNull => (TAG_NULL, 0),
                        Op::ReturnReg => {
                            let src = op.a as usize;
                            let rt = t[src];
                            if rt == TAG_UNINIT {
                                break 'frame_loop;
                            }
                            let rb = v[src];
                            if rt == TAG_HEAP {
                                let is_ref = heap
                                    .get(rb as usize)
                                    .and_then(Option::as_ref)
                                    .is_none_or(Value::is_reference);
                                if is_ref {
                                    break 'frame_loop;
                                }
                            }
                            (rt, rb)
                        }
                        _ => unreachable!(),
                    };

                    if let Some(mask) = cur_fn.return_mask
                        && !return_mask_fits(mask, ret_tag, ret_bit, heap)
                    {
                        break 'frame_loop;
                    }

                    // Returning from a nested FastFrame?
                    if let Some(caller_fast) = fast_stack.frames.pop() {
                        // Detach the returned heap slot from the dying frame so
                        // `release_frame_heap` does not drop it.
                        if ret_tag == TAG_HEAP && op.op == Op::ReturnReg {
                            t[op.a as usize] = TAG_UNINIT;
                        }
                        if cur_has_heap {
                            fast_stack.release_frame_heap(base, cur_reg_count as usize);
                        }
                        let caller_base = caller_fast.base as usize;
                        let caller_dst = caller_fast.return_dst as usize;
                        let mut caller_has_heap = caller_fast.has_heap;

                        // Write return value into caller's window.
                        if fast_stack.tags[caller_base + caller_dst] == TAG_HEAP {
                            let old_idx = fast_stack.bits[caller_base + caller_dst] as u32;
                            fast_stack.free_heap_slot(old_idx);
                        }
                        fast_stack.tags[caller_base + caller_dst] = ret_tag;
                        fast_stack.bits[caller_base + caller_dst] = ret_bit;
                        if ret_tag == TAG_HEAP {
                            caller_has_heap = true;
                        }

                        if caller_fast.function != cur_fn_idx {
                            cur_fn_idx = caller_fast.function;
                            cur_fn = match &fast_table[cur_fn_idx.index()] {
                                FastEntry::Compiled(f) => f.as_ref(),
                                _ => unreachable!("caller frame must be compiled"),
                            };
                            cur_ops = &cur_fn.ops[..];
                            cur_f64 = &cur_fn.f64_consts[..];
                            cur_local_count = cur_fn.local_count;
                            cur_reg_count = cur_fn.reg_count;
                        }
                        cur_has_heap = caller_has_heap;
                        base = caller_base;
                        ip = caller_fast.return_fast_ip as usize;
                        continue 'frame_loop;
                    }

                    // Returning from the entry frame F0!
                    if !entry_can_fast_return {
                        break 'frame_loop;
                    }
                    let Some(caller_vm) = stack.caller().copied() else {
                        break 'frame_loop;
                    };
                    let Some(destination) = entry_frame.destination() else {
                        break 'frame_loop;
                    };
                    if ret_tag == TAG_HEAP && op.op == Op::ReturnReg {
                        t[op.a as usize] = TAG_UNINIT;
                    }
                    let Some(ret_value) = fast_stack.take_value(ret_tag, ret_bit) else {
                        break 'frame_loop;
                    };
                    fast_stack.reset();
                    stack.pop_for_return();
                    registers.truncate(entry_frame.start());
                    registers.put(caller_vm.base() + destination.index(), ret_value);
                    *work_io = work;
                    *executed_io += executed;
                    return FastExit::Returned(Position {
                        frame: caller_vm,
                        ip: caller_vm.ip(),
                    });
                }

                // =============================================================
                // EXACT DEOPTIMIZATION
                // =============================================================
                Op::Deopt => {
                    break 'frame_loop;
                }
            }
        }
    }

    materialize_deopt(
        stack,
        registers,
        fast_stack,
        entry_frame,
        entry_fast.reg_count as usize,
        cur_fn_idx,
        cur_fn,
        base,
        ip,
        false,
        work_io,
        executed_io,
        work,
        executed,
    )
}

/// Materialize the entry frame `F0` and every active nested [`FastFrame`]
/// back into `Vm::registers` and `Vm::call_stack` at the exact bytecode IP.
#[cold]
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn materialize_deopt(
    stack: &mut CallStack,
    registers: &mut Registers,
    fast_stack: &mut FastStack,
    entry_frame: Frame,
    entry_reg_count: usize,
    cur_fn_idx: FunctionIndex,
    cur_fn: &FastFunction,
    cur_base: usize,
    cur_fast_ip: usize,
    yield_now: bool,
    work_io: &mut usize,
    executed_io: &mut u64,
    work: usize,
    executed: u64,
) -> FastExit {
    let cur_bc_ip = cur_fn
        .bc_ip
        .get(cur_fast_ip)
        .copied()
        .map(|i| i as usize)
        .unwrap_or(cur_fn.bc_to_fast.len());

    // 1. Restore entry frame F0's registers back to its VM window.
    fast_stack.store_frame_to_vm(registers, 0, entry_frame.base(), entry_reg_count);

    // 2. If nested FastFrames exist, materialize each one onto `CallStack`
    //    and `Registers` in bottom-up order.
    let final_frame = if fast_stack.frames.is_empty() {
        let mut frame = entry_frame;
        frame.set_ip(cur_bc_ip);
        if let Some(current) = stack.current_mut() {
            current.set_ip(cur_bc_ip);
        }
        frame
    } else {
        let saved: Vec<FastFrame> = std::mem::take(&mut fast_stack.frames);
        if let Some(current) = stack.current_mut() {
            current.set_ip(saved[0].return_bc_ip as usize);
        }
        for k in 1..saved.len() {
            let f = saved[k];
            let prev_dst = saved[k - 1].return_dst;
            let vm_base = registers.len();
            registers.grow_to(vm_base + f.reg_count as usize);
            fast_stack.store_frame_to_vm(registers, f.base as usize, vm_base, f.reg_count as usize);
            let mut vm_frame = Frame::new(f.function, vm_base, Some(Reg(u16::from(prev_dst))));
            vm_frame.set_ip(f.return_bc_ip as usize);
            stack.push(vm_frame);
        }
        let last_dst = saved.last().expect("non-empty saved frames").return_dst;
        let inner_vm_base = registers.len();
        registers.grow_to(inner_vm_base + cur_fn.reg_count as usize);
        fast_stack.store_frame_to_vm(
            registers,
            cur_base,
            inner_vm_base,
            cur_fn.reg_count as usize,
        );
        let mut inner_frame = Frame::new(cur_fn_idx, inner_vm_base, Some(Reg(u16::from(last_dst))));
        inner_frame.set_ip(cur_bc_ip);
        stack.push(inner_frame);
        inner_frame
    };

    fast_stack.reset();
    *work_io = work;
    *executed_io += executed;
    FastExit::Deopt {
        position: Position {
            frame: final_frame,
            ip: cur_bc_ip,
        },
        yield_now,
    }
}
