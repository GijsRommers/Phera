//! Tier-2 compact register VM execution engine (100% safe Rust).
//!
//! Runs 8-byte [`FastOp`] streams over raw `(tags: &mut [u8; 256], bits: &mut [u64; 256])`
//! windows (zero bounds checks, raw 64-bit `i64`/`f64`, zero-alloc `TAG_CONST`),
//! dispatching pure-scalar inner loops to [`scalar_loop::try_run_scalar_loop`]
//! and pure-integer self-recursion to [`int_rec::run_int_rec_entry`].
//! All slow/heap/array/string/deopt paths live in out-of-line helpers and a
//! single `materialize_deopt` exit at the end of `run_fast`.

use std::rc::Rc;
use std::sync::atomic::Ordering;

use phera_bytecode::{Reg, TypeMask};
use phera_value::{PhpArray, PhpString, Value};

use crate::calls::call_stack::CallStack;
use crate::calls::execution_loop::{Position, YIELD_AFTER_STEPS};
use crate::calls::frame::Frame;
use crate::calls::prepared_call::CallKind;
use crate::calls::registers::Registers;
use crate::code_table::{CodeTable, FunctionIndex};
use crate::fast_tier::int_rec::run_int_rec_entry;
use crate::fast_tier::scalar_loop::{ScalarExit, try_run_scalar_loop};
use crate::fast_tier::stack::{FastFrame, FastStack};
use crate::fast_tier::{
    CMP_EQ, CMP_GE, CMP_GT, CMP_IDENT, CMP_LE, CMP_LT, CMP_NE, CMP_NOT_IDENT, FastEntry, FastExit,
    FastFunction, Op, TAG_BOOL, TAG_CONST, TAG_FLOAT, TAG_HEAP, TAG_INT, TAG_NULL, TAG_UNINIT,
};

const NUM_ADD: u8 = 0;
const NUM_SUB: u8 = 1;
const NUM_MUL: u8 = 2;
const NUM_DIV: u8 = 3;

#[inline(always)]
fn scalar_truthy(tag: u8, bit: u64) -> Option<bool> {
    match tag {
        TAG_NULL => Some(false),
        TAG_BOOL | TAG_INT => Some(bit != 0),
        TAG_FLOAT => Some(f64::from_bits(bit) != 0.0),
        _ => None,
    }
}

#[inline(always)]
fn tag_type_mask(tag: u8, bit: u64, heap: &[Option<Value>], consts: &[Value]) -> Option<TypeMask> {
    match tag {
        TAG_NULL => Some(TypeMask::NULL),
        TAG_BOOL => Some(if bit != 0 {
            TypeMask::TRUE
        } else {
            TypeMask::FALSE
        }),
        TAG_INT => Some(TypeMask::INT),
        TAG_FLOAT => Some(TypeMask::FLOAT),
        TAG_CONST => {
            let val = consts.get(bit as usize)?;
            Some(crate::types::own_type(val))
        }
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
fn return_mask_fits(
    mask: TypeMask,
    tag: u8,
    bit: u64,
    heap: &[Option<Value>],
    consts: &[Value],
) -> bool {
    if tag == TAG_NULL && mask.intersects(TypeMask::VOID) {
        return true;
    }
    tag_type_mask(tag, bit, heap, consts).is_some_and(|own| mask.intersects(own))
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
    if lt == TAG_FLOAT && rt == TAG_FLOAT {
        let a = f64::from_bits(lb);
        let b = f64::from_bits(rb);
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
    compare_nums_slow(cmp, lt, lb, rt, rb)
}

#[inline(never)]
fn compare_nums_slow(cmp: u8, lt: u8, lb: u64, rt: u8, rb: u64) -> Option<bool> {
    if (lt == TAG_INT || lt == TAG_FLOAT) && (rt == TAG_INT || rt == TAG_FLOAT) {
        if cmp == CMP_IDENT {
            return Some(false);
        }
        if cmp == CMP_NOT_IDENT {
            return Some(true);
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
        && lt < TAG_CONST
        && rt < TAG_CONST
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

#[cold]
#[inline(never)]
fn release_dst_slot(
    idx: u32,
    dst: usize,
    local_count: u8,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
) -> bool {
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
    true
}

#[inline(always)]
#[allow(clippy::too_many_arguments)]
fn write_scalar(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    dst: usize,
    tag: u8,
    bit: u64,
    local_count: u8,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
) -> bool {
    if t[dst] == TAG_HEAP && !release_dst_slot(v[dst] as u32, dst, local_count, heap, free_heap) {
        return false;
    }
    t[dst] = tag;
    v[dst] = bit;
    true
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn write_heap(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    dst: usize,
    value: Value,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
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

#[inline(always)]
fn read_owned_value(tag: u8, bit: u64, heap: &[Option<Value>], consts: &[Value]) -> Option<Value> {
    match tag {
        TAG_INT => Some(Value::Int(bit as i64)),
        TAG_FLOAT => Some(Value::Float(f64::from_bits(bit))),
        TAG_BOOL => Some(Value::Bool(bit != 0)),
        TAG_NULL => Some(Value::Null),
        TAG_CONST => consts.get(bit as usize).cloned(),
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

#[inline(never)]
fn num_binary_slow(kind: u8, lt: u8, lb: u64, rt: u8, rb: u64) -> Option<(u8, u64)> {
    if lt == TAG_INT && rt == TAG_INT {
        let a = lb as i64;
        let b = rb as i64;
        return match kind {
            NUM_ADD => Some(match a.checked_add(b) {
                Some(s) => (TAG_INT, s as u64),
                None => (TAG_FLOAT, ((a as f64) + (b as f64)).to_bits()),
            }),
            NUM_SUB => Some(match a.checked_sub(b) {
                Some(s) => (TAG_INT, s as u64),
                None => (TAG_FLOAT, ((a as f64) - (b as f64)).to_bits()),
            }),
            NUM_MUL => Some(match a.checked_mul(b) {
                Some(p) => (TAG_INT, p as u64),
                None => (TAG_FLOAT, ((a as f64) * (b as f64)).to_bits()),
            }),
            NUM_DIV => {
                if b == 0 {
                    return None;
                }
                if a == i64::MIN && b == -1 {
                    Some((TAG_FLOAT, (-(i64::MIN as f64)).to_bits()))
                } else if a % b == 0 {
                    Some((TAG_INT, (a / b) as u64))
                } else {
                    Some((TAG_FLOAT, ((a as f64) / (b as f64)).to_bits()))
                }
            }
            _ => None,
        };
    }
    if (lt != TAG_INT && lt != TAG_FLOAT) || (rt != TAG_INT && rt != TAG_FLOAT) {
        return None;
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
    let res = match kind {
        NUM_ADD => a + b,
        NUM_SUB => a - b,
        NUM_MUL => a * b,
        NUM_DIV => {
            if b == 0.0 {
                return None;
            }
            a / b
        }
        _ => return None,
    };
    Some((TAG_FLOAT, res.to_bits()))
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn move_heap_slow(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    dst: usize,
    src_idx: u64,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    let Some(val) = read_owned_value(TAG_HEAP, src_idx, heap, consts) else {
        return false;
    };
    write_heap(t, v, dst, val, local_count, has_heap, heap, free_heap)
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn assign_take_heap_slow(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    dst: usize,
    src: usize,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
) -> bool {
    let src_idx = v[src] as usize;
    if heap
        .get(src_idx)
        .and_then(Option::as_ref)
        .is_none_or(Value::is_reference)
    {
        return false;
    }
    if t[dst] == TAG_HEAP
        && dst < local_count as usize
        && heap
            .get(v[dst] as usize)
            .and_then(Option::as_ref)
            .is_some_and(Value::is_reference)
    {
        return false;
    }
    if dst != src {
        if t[dst] == TAG_HEAP
            && let Some(slot) = heap.get_mut(v[dst] as usize)
        {
            *slot = None;
            free_heap.push(v[dst] as u32);
        }
        t[dst] = TAG_HEAP;
        v[dst] = v[src];
        t[src] = TAG_UNINIT;
        *has_heap = true;
    }
    true
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn str_append_helper(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    loc: usize,
    src: usize,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    let lt = t[loc];
    let rt = t[src];
    let mut stack_buf = [0u8; 32];
    let rhs_bytes: &[u8] = if rt == TAG_CONST {
        match consts.get(v[src] as usize) {
            Some(Value::InternedStr(s)) => s.as_bytes(),
            Some(Value::String(s)) => s.as_bytes(),
            _ => return false,
        }
    } else if rt == TAG_HEAP {
        let r_idx = v[src] as usize;
        if lt == TAG_HEAP && (v[loc] as usize) == r_idx {
            return false;
        }
        match heap.get(r_idx) {
            Some(Some(Value::InternedStr(s))) => s.as_bytes(),
            Some(Some(Value::String(s))) => {
                let bytes = s.as_bytes().to_vec();
                return str_append_owned_bytes(
                    t, v, loc, &bytes, has_heap, heap, free_heap, consts,
                );
            }
            _ => return false,
        }
    } else if rt == TAG_INT {
        let n = v[src] as i64;
        let neg = n < 0;
        let mut mag = n.unsigned_abs();
        let mut pos = stack_buf.len();
        loop {
            pos -= 1;
            stack_buf[pos] = b'0' + (mag % 10) as u8;
            mag /= 10;
            if mag == 0 {
                break;
            }
        }
        if neg {
            pos -= 1;
            stack_buf[pos] = b'-';
        }
        &stack_buf[pos..]
    } else {
        return false;
    };
    str_append_owned_bytes(t, v, loc, rhs_bytes, has_heap, heap, free_heap, consts)
}

#[allow(clippy::too_many_arguments)]
fn str_append_owned_bytes(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    loc: usize,
    rhs_bytes: &[u8],
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    let lt = t[loc];
    if lt == TAG_HEAP {
        let idx = v[loc] as usize;
        match heap.get_mut(idx) {
            Some(Some(Value::String(s))) => {
                s.push_bytes(rhs_bytes);
                true
            }
            Some(Some(slot @ Value::InternedStr(_))) => {
                let Value::InternedStr(s) = *slot else {
                    unreachable!()
                };
                let mut buf = Vec::with_capacity(s.len() + rhs_bytes.len());
                buf.extend_from_slice(s.as_bytes());
                buf.extend_from_slice(rhs_bytes);
                *slot = Value::String(PhpString::from(buf));
                true
            }
            _ => false,
        }
    } else if lt == TAG_CONST {
        let lhs_bytes = match consts.get(v[loc] as usize) {
            Some(Value::InternedStr(s)) => s.as_bytes(),
            Some(Value::String(s)) => s.as_bytes(),
            _ => return false,
        };
        let mut buf = Vec::with_capacity(lhs_bytes.len() + rhs_bytes.len());
        buf.extend_from_slice(lhs_bytes);
        buf.extend_from_slice(rhs_bytes);
        let idx = if let Some(slot) = free_heap.pop() {
            heap[slot as usize] = Some(Value::String(PhpString::from(buf)));
            slot
        } else {
            let slot = heap.len() as u32;
            heap.push(Some(Value::String(PhpString::from(buf))));
            slot
        };
        t[loc] = TAG_HEAP;
        v[loc] = u64::from(idx);
        *has_heap = true;
        true
    } else {
        false
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn new_array_helper(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    dst: usize,
    cap: i32,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
) -> bool {
    let array = if cap > 0 {
        PhpArray::with_capacity(cap as usize)
    } else {
        PhpArray::new()
    };
    write_heap(
        t,
        v,
        dst,
        Value::Array(Rc::new(array)),
        local_count,
        has_heap,
        heap,
        free_heap,
    )
}

/// Helper to read a `PhpArray` int slot using direct `list_slots()` fast path.
#[inline(always)]
fn array_get_int_fast(arr: &PhpArray, key: i64) -> Option<&Value> {
    if let Some((start, slots)) = arr.list_slots() {
        let off = key.wrapping_sub(start);
        if off >= 0 && (off as usize) < slots.len() {
            return slots[off as usize].as_ref();
        }
        return None;
    }
    arr.get_int(key)
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_get_int_helper(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    dst: usize,
    arr_r: usize,
    idx_r: usize,
    quiet: bool,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    if t[idx_r] != TAG_INT {
        return false;
    }
    let key = v[idx_r] as i64;
    let bt = t[arr_r];

    enum Found {
        Scalar(u8, u64),
        Heap(Value),
        Bail,
    }

    let resolve_val = |val: &Value| -> Found {
        match val {
            Value::Int(i) => Found::Scalar(TAG_INT, *i as u64),
            Value::Float(f) => Found::Scalar(TAG_FLOAT, f.to_bits()),
            Value::Bool(b) => Found::Scalar(TAG_BOOL, u64::from(*b)),
            Value::Null => Found::Scalar(TAG_NULL, 0),
            other if !other.is_reference() => Found::Heap(other.clone()),
            _ => Found::Bail,
        }
    };

    let resolve_bytes = |bytes: &[u8]| -> Found {
        if key >= 0 && (key as usize) < bytes.len() {
            Found::Heap(Value::String(PhpString::from(vec![bytes[key as usize]])))
        } else {
            Found::Bail
        }
    };

    let found = if bt == TAG_HEAP {
        match heap.get(v[arr_r] as usize) {
            Some(Some(Value::Array(rc))) => match array_get_int_fast(rc, key) {
                Some(val) => resolve_val(val),
                None if quiet => Found::Scalar(TAG_NULL, 0),
                None => Found::Bail,
            },
            Some(Some(Value::InternedStr(s))) => resolve_bytes(s.as_bytes()),
            Some(Some(Value::String(s))) => resolve_bytes(s.as_bytes()),
            Some(Some(Value::Reference(cell))) => {
                if let Ok(guard) = cell.try_borrow() {
                    match &*guard {
                        Value::Array(rc) => match array_get_int_fast(rc, key) {
                            Some(val) => resolve_val(val),
                            None if quiet => Found::Scalar(TAG_NULL, 0),
                            None => Found::Bail,
                        },
                        _ => Found::Bail,
                    }
                } else {
                    Found::Bail
                }
            }
            _ => Found::Bail,
        }
    } else if bt == TAG_CONST {
        match consts.get(v[arr_r] as usize) {
            Some(Value::InternedStr(s)) => resolve_bytes(s.as_bytes()),
            Some(Value::String(s)) => resolve_bytes(s.as_bytes()),
            Some(Value::Array(rc)) => match array_get_int_fast(rc, key) {
                Some(val) => resolve_val(val),
                None if quiet => Found::Scalar(TAG_NULL, 0),
                None => Found::Bail,
            },
            _ => Found::Bail,
        }
    } else {
        Found::Bail
    };

    match found {
        Found::Scalar(tag, bit) => write_scalar(t, v, dst, tag, bit, local_count, heap, free_heap),
        Found::Heap(val) => write_heap(t, v, dst, val, local_count, has_heap, heap, free_heap),
        Found::Bail => false,
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_2d_get_int_helper(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    dst: usize,
    arr_r: usize,
    idx1_r: usize,
    idx2_r: usize,
    quiet: bool,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
) -> bool {
    if t[arr_r] != TAG_HEAP || t[idx1_r] != TAG_INT || t[idx2_r] != TAG_INT {
        return false;
    }
    let k1 = v[idx1_r] as i64;
    let k2 = v[idx2_r] as i64;

    enum Found2 {
        Scalar(u8, u64),
        Heap(Value),
        Bail,
    }

    let found = match heap.get(v[arr_r] as usize) {
        Some(Some(Value::Array(rc1))) => match array_get_int_fast(rc1, k1) {
            Some(Value::Array(rc2)) => match array_get_int_fast(rc2, k2) {
                Some(Value::Int(i)) => Found2::Scalar(TAG_INT, *i as u64),
                Some(Value::Float(f)) => Found2::Scalar(TAG_FLOAT, f.to_bits()),
                Some(Value::Bool(b)) => Found2::Scalar(TAG_BOOL, u64::from(*b)),
                Some(Value::Null) => Found2::Scalar(TAG_NULL, 0),
                Some(other) if !other.is_reference() => Found2::Heap(other.clone()),
                None if quiet => Found2::Scalar(TAG_NULL, 0),
                _ => Found2::Bail,
            },
            None if quiet => Found2::Scalar(TAG_NULL, 0),
            _ => Found2::Bail,
        },
        _ => Found2::Bail,
    };

    match found {
        Found2::Scalar(tag, bit) => write_scalar(t, v, dst, tag, bit, local_count, heap, free_heap),
        Found2::Heap(val) => write_heap(t, v, dst, val, local_count, has_heap, heap, free_heap),
        Found2::Bail => false,
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_set_int_helper(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    arr_r: usize,
    idx_r: usize,
    src_r: usize,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    if t[idx_r] != TAG_INT {
        return false;
    }
    let key = v[idx_r] as i64;
    let Some(val) = read_owned_value(t[src_r], v[src_r], heap, consts) else {
        return false;
    };
    // PHP auto-vivification: `$arr[$i] = $v` on undefined / `null` creates a new array!
    if t[arr_r] == TAG_UNINIT || t[arr_r] == TAG_NULL {
        let mut arr = PhpArray::new();
        if arr.insert_int_if_not_ref(key, val).is_err() {
            return false;
        }
        let slot = if let Some(free) = free_heap.pop() {
            heap[free as usize] = Some(Value::Array(Rc::new(arr)));
            free
        } else {
            let s = heap.len() as u32;
            heap.push(Some(Value::Array(Rc::new(arr))));
            s
        };
        t[arr_r] = TAG_HEAP;
        v[arr_r] = u64::from(slot);
        *has_heap = true;
        return true;
    }
    if t[arr_r] != TAG_HEAP {
        return false;
    }
    let set_in = |rc: &mut Rc<PhpArray>, val: Value| -> bool {
        let arr = Rc::make_mut(rc);
        if let Some((start, slots)) = arr.list_slots_mut() {
            let off = key.wrapping_sub(start);
            if off >= 0
                && (off as usize) < slots.len()
                && let Some(existing) = &mut slots[off as usize]
            {
                if !existing.is_reference() {
                    *existing = val;
                    return true;
                }
                return false;
            }
        }
        arr.insert_int_if_not_ref(key, val).is_ok()
    };
    match heap.get_mut(v[arr_r] as usize) {
        Some(Some(Value::Array(rc))) => set_in(rc, val),
        Some(Some(Value::Reference(cell))) => {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                match &mut *guard {
                    Value::Array(rc) => set_in(rc, val),
                    _ => false,
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

/// 2D integer-indexed array write `$arr[$i1][$i2] = $src` (with auto-vivification).
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_2d_set_int_helper(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    arr_r: usize,
    i1_r: usize,
    i2_r: usize,
    src_r: usize,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    if t[i1_r] != TAG_INT || t[i2_r] != TAG_INT {
        return false;
    }
    let k1 = v[i1_r] as i64;
    let k2 = v[i2_r] as i64;
    let Some(val) = read_owned_value(t[src_r], v[src_r], heap, consts) else {
        return false;
    };
    if t[arr_r] == TAG_UNINIT || t[arr_r] == TAG_NULL {
        let mut inner = PhpArray::new();
        if inner.insert_int_if_not_ref(k2, val).is_err() {
            return false;
        }
        let mut outer = PhpArray::new();
        if outer
            .insert_int_if_not_ref(k1, Value::Array(Rc::new(inner)))
            .is_err()
        {
            return false;
        }
        let slot = if let Some(free) = free_heap.pop() {
            heap[free as usize] = Some(Value::Array(Rc::new(outer)));
            free
        } else {
            let s = heap.len() as u32;
            heap.push(Some(Value::Array(Rc::new(outer))));
            s
        };
        t[arr_r] = TAG_HEAP;
        v[arr_r] = u64::from(slot);
        *has_heap = true;
        return true;
    }
    if t[arr_r] != TAG_HEAP {
        return false;
    }
    let Some(Some(Value::Array(outer_rc))) = heap.get_mut(v[arr_r] as usize) else {
        return false;
    };
    let outer = Rc::make_mut(outer_rc);
    if let Some(slot) = outer.get_mut_int(k1) {
        match slot {
            Value::Array(inner_rc) => {
                let inner = Rc::make_mut(inner_rc);
                if let Some((start, slots)) = inner.list_slots_mut() {
                    let off = k2.wrapping_sub(start);
                    if off >= 0
                        && (off as usize) < slots.len()
                        && let Some(existing) = &mut slots[off as usize]
                    {
                        if !existing.is_reference() {
                            *existing = val;
                            return true;
                        }
                        return false;
                    }
                }
                inner.insert_int_if_not_ref(k2, val).is_ok()
            }
            Value::Null => {
                let mut inner = PhpArray::new();
                if inner.insert_int_if_not_ref(k2, val).is_err() {
                    return false;
                }
                *slot = Value::Array(Rc::new(inner));
                true
            }
            _ => false,
        }
    } else {
        let mut inner = PhpArray::new();
        if inner.insert_int_if_not_ref(k2, val).is_err() {
            return false;
        }
        outer
            .insert_int_if_not_ref(k1, Value::Array(Rc::new(inner)))
            .is_ok()
    }
}

#[inline(never)]
fn arr_add_assign_int_helper(
    t: &[u8; 256],
    v: &[u64; 256],
    arr_r: usize,
    idx_r: usize,
    src_r: usize,
    heap: &mut [Option<Value>],
) -> bool {
    if t[arr_r] != TAG_HEAP || t[idx_r] != TAG_INT {
        return false;
    }
    let st = t[src_r];
    if st != TAG_INT && st != TAG_FLOAT {
        return false;
    }
    let key = v[idx_r] as i64;
    let rhs_bit = v[src_r];
    let Some(Some(Value::Array(rc))) = heap.get_mut(v[arr_r] as usize) else {
        return false;
    };
    let target = Rc::make_mut(rc);
    let Some(slot) = target.get_mut_int(key) else {
        return false;
    };
    match (slot, st) {
        (slot @ Value::Int(_), TAG_INT) => {
            let Value::Int(lhs) = *slot else {
                unreachable!()
            };
            let rhs = rhs_bit as i64;
            match lhs.checked_add(rhs) {
                Some(sum) => *slot = Value::Int(sum),
                None => *slot = Value::Float((lhs as f64) + (rhs as f64)),
            }
            true
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
            *slot = Value::Float((lhs as f64) + f64::from_bits(rhs_bit));
            true
        }
        _ => false,
    }
}

#[inline(never)]
fn arr_add_from_arr_helper(
    t: &[u8; 256],
    v: &[u64; 256],
    dst_arr_r: usize,
    src_arr_r: usize,
    idx_r: usize,
    heap: &mut [Option<Value>],
) -> bool {
    if t[dst_arr_r] != TAG_HEAP || t[src_arr_r] != TAG_HEAP || t[idx_r] != TAG_INT {
        return false;
    }
    let key = v[idx_r] as i64;
    enum RhsNum {
        Int(i64),
        Float(f64),
    }
    let rhs = match heap.get(v[src_arr_r] as usize) {
        Some(Some(Value::Array(rc))) => match array_get_int_fast(rc, key) {
            Some(Value::Int(i)) => RhsNum::Int(*i),
            Some(Value::Float(f)) => RhsNum::Float(*f),
            _ => return false,
        },
        _ => return false,
    };
    let Some(Some(Value::Array(rc))) = heap.get_mut(v[dst_arr_r] as usize) else {
        return false;
    };
    let target = Rc::make_mut(rc);
    let Some(slot) = target.get_mut_int(key) else {
        return false;
    };
    match (slot, rhs) {
        (slot @ Value::Int(_), RhsNum::Int(rv)) => {
            let Value::Int(lv) = *slot else {
                unreachable!()
            };
            match lv.checked_add(rv) {
                Some(sum) => *slot = Value::Int(sum),
                None => *slot = Value::Float((lv as f64) + (rv as f64)),
            }
            true
        }
        (slot @ Value::Float(_), RhsNum::Float(rv)) => {
            let Value::Float(lv) = *slot else {
                unreachable!()
            };
            *slot = Value::Float(lv + rv);
            true
        }
        (slot @ Value::Float(_), RhsNum::Int(rv)) => {
            let Value::Float(lv) = *slot else {
                unreachable!()
            };
            *slot = Value::Float(lv + (rv as f64));
            true
        }
        (slot @ Value::Int(_), RhsNum::Float(rv)) => {
            let Value::Int(lv) = *slot else {
                unreachable!()
            };
            *slot = Value::Float((lv as f64) + rv);
            true
        }
        _ => false,
    }
}

enum ArrayLoopExit {
    Done(usize),
    Yield,
    Bail(usize),
}

/// Hoisted-slice inner loop for `for ($j = ...; $j >= lim; $j--) $Y[$j] += $X[$j]`.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_add_from_arr_loop_helper(
    t: &[u8; 256],
    v: &mut [u64; 256],
    dst_arr_r: usize,
    src_arr_r: usize,
    idx_r: usize,
    lim: i64,
    heap: &mut [Option<Value>],
    budget_start: usize,
) -> ArrayLoopExit {
    if t[dst_arr_r] != TAG_HEAP || t[src_arr_r] != TAG_HEAP || t[idx_r] != TAG_INT {
        return ArrayLoopExit::Bail(0);
    }
    let dst_slot = v[dst_arr_r] as usize;
    let src_slot = v[src_arr_r] as usize;
    let mut j = v[idx_r] as i64;
    let mut budget = budget_start.max(1);

    // Distinct `Storage::List` arrays: borrow both slices ONCE outside the loop!
    if dst_slot != src_slot && dst_slot < heap.len() && src_slot < heap.len() {
        let (dst_opt, src_opt) = if dst_slot < src_slot {
            let (left, right) = heap.split_at_mut(src_slot);
            (&mut left[dst_slot], &right[0])
        } else {
            let (left, right) = heap.split_at_mut(dst_slot);
            (&mut right[0], &left[src_slot])
        };
        if let (Some(Value::Array(dst_rc)), Some(Value::Array(src_rc))) = (dst_opt, src_opt) {
            let dst_arr = Rc::make_mut(dst_rc);
            if let (Some((d_start, d_slots)), Some((s_start, s_slots))) =
                (dst_arr.list_slots_mut(), src_rc.list_slots())
            {
                loop {
                    let d_off = j.wrapping_sub(d_start);
                    let s_off = j.wrapping_sub(s_start);
                    if d_off < 0
                        || s_off < 0
                        || (d_off as usize) >= d_slots.len()
                        || (s_off as usize) >= s_slots.len()
                    {
                        v[idx_r] = j as u64;
                        return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
                    }
                    match (&mut d_slots[d_off as usize], &s_slots[s_off as usize]) {
                        (Some(Value::Int(lv)), Some(Value::Int(rv))) => {
                            if let Some(sum) = lv.checked_add(*rv) {
                                *lv = sum;
                            } else {
                                d_slots[d_off as usize] =
                                    Some(Value::Float((*lv as f64) + (*rv as f64)));
                            }
                        }
                        (Some(Value::Float(lv)), Some(Value::Float(rv))) => {
                            *lv += *rv;
                        }
                        (Some(Value::Float(lv)), Some(Value::Int(rv))) => {
                            *lv += *rv as f64;
                        }
                        (Some(Value::Int(lv)), Some(Value::Float(rv))) => {
                            let res = (*lv as f64) + *rv;
                            d_slots[d_off as usize] = Some(Value::Float(res));
                        }
                        _ => {
                            v[idx_r] = j as u64;
                            return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
                        }
                    }
                    let Some(next_j) = j.checked_sub(1) else {
                        v[idx_r] = j as u64;
                        return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
                    };
                    j = next_j;
                    if j < lim {
                        v[idx_r] = j as u64;
                        return ArrayLoopExit::Done(budget_start.saturating_sub(budget));
                    }
                    budget -= 1;
                    if budget == 0 {
                        v[idx_r] = j as u64;
                        return ArrayLoopExit::Yield;
                    }
                }
            }
        }
    }

    // Fallback (aliased arrays or `Storage::Map`): run one iteration at a time.
    loop {
        v[idx_r] = j as u64;
        if !arr_add_from_arr_helper(t, v, dst_arr_r, src_arr_r, idx_r, heap) {
            return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
        }
        let Some(next_j) = j.checked_sub(1) else {
            return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
        };
        j = next_j;
        if j < lim {
            v[idx_r] = j as u64;
            return ArrayLoopExit::Done(budget_start.saturating_sub(budget));
        }
        budget -= 1;
        if budget == 0 {
            v[idx_r] = j as u64;
            return ArrayLoopExit::Yield;
        }
    }
}

/// Hoisted-slice inner loop for `for ($k = ...; $k <= lim; $k += $step) unset($flags[$k])`.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_unset_step_loop_helper(
    t: &[u8; 256],
    v: &mut [u64; 256],
    arr_r: usize,
    k_r: usize,
    step_r: usize,
    lim: i64,
    heap: &mut [Option<Value>],
    budget_start: usize,
) -> ArrayLoopExit {
    if t[arr_r] != TAG_HEAP || t[k_r] != TAG_INT || t[step_r] != TAG_INT {
        return ArrayLoopExit::Bail(0);
    }
    let step = v[step_r] as i64;
    let mut k = v[k_r] as i64;
    let mut budget = budget_start.max(1);

    if let Some(Some(Value::Array(rc))) = heap.get_mut(v[arr_r] as usize) {
        let arr = Rc::make_mut(rc);
        if let Some((start, slots, dead)) = arr.list_slots_mut_with_dead() {
            loop {
                let off = k.wrapping_sub(start);
                if off >= 0 && (off as usize) < slots.len() && slots[off as usize].take().is_some()
                {
                    *dead += 1;
                }
                let Some(next_k) = k.checked_add(step) else {
                    v[k_r] = k as u64;
                    return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
                };
                k = next_k;
                if k > lim {
                    v[k_r] = k as u64;
                    return ArrayLoopExit::Done(budget_start.saturating_sub(budget));
                }
                budget -= 1;
                if budget == 0 {
                    v[k_r] = k as u64;
                    return ArrayLoopExit::Yield;
                }
            }
        }
    }
    ArrayLoopExit::Bail(0)
}

/// Hoisted-slice inner loop for `for ($k = ...; $k <= lim; $k += $step) $arr[$k] = $val`.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_set_reg_step_loop_helper(
    t: &[u8; 256],
    v: &mut [u64; 256],
    arr_r: usize,
    k_r: usize,
    step_r: usize,
    packed_imm: i32,
    heap: &mut [Option<Value>],
    consts: &[Value],
    budget_start: usize,
) -> ArrayLoopExit {
    if t[arr_r] != TAG_HEAP || t[k_r] != TAG_INT || t[step_r] != TAG_INT {
        return ArrayLoopExit::Bail(0);
    }
    let lim = i64::from(packed_imm & 0x003f_ffff);
    let is_const = (packed_imm & (1 << 23)) != 0;
    let payload = ((packed_imm >> 24) & 0xff) as u8;
    let val = if is_const {
        Value::Int(i64::from(payload as i8))
    } else {
        let r = payload as usize;
        let Some(v_owned) = read_owned_value(t[r], v[r], heap, consts) else {
            return ArrayLoopExit::Bail(0);
        };
        v_owned
    };
    let step = v[step_r] as i64;
    let mut k = v[k_r] as i64;
    let mut budget = budget_start.max(1);

    if let Some(Some(Value::Array(rc))) = heap.get_mut(v[arr_r] as usize) {
        let arr = Rc::make_mut(rc);
        if let Some((start, slots)) = arr.list_slots_mut() {
            loop {
                let off = k.wrapping_sub(start);
                if off < 0 || (off as usize) >= slots.len() {
                    v[k_r] = k as u64;
                    return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
                }
                match &mut slots[off as usize] {
                    Some(existing) if !existing.is_reference() => {
                        *existing = val.clone();
                    }
                    _ => {
                        v[k_r] = k as u64;
                        return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
                    }
                }
                let Some(next_k) = k.checked_add(step) else {
                    v[k_r] = k as u64;
                    return ArrayLoopExit::Bail(budget_start.saturating_sub(budget));
                };
                k = next_k;
                if k > lim {
                    v[k_r] = k as u64;
                    return ArrayLoopExit::Done(budget_start.saturating_sub(budget));
                }
                budget -= 1;
                if budget == 0 {
                    v[k_r] = k as u64;
                    return ArrayLoopExit::Yield;
                }
            }
        }
    }
    ArrayLoopExit::Bail(0)
}

#[inline(never)]
fn arr_push_helper(
    t: &[u8; 256],
    v: &[u64; 256],
    arr_r: usize,
    src_r: usize,
    heap: &mut [Option<Value>],
    consts: &[Value],
) -> bool {
    if t[arr_r] != TAG_HEAP {
        return false;
    }
    let Some(val) = read_owned_value(t[src_r], v[src_r], heap, consts) else {
        return false;
    };
    match heap.get_mut(v[arr_r] as usize) {
        Some(Some(Value::Array(rc))) => Rc::make_mut(rc).push(val).is_ok(),
        _ => false,
    }
}

#[inline(never)]
fn arr_unset_int_helper(
    t: &[u8; 256],
    v: &[u64; 256],
    arr_r: usize,
    idx_r: usize,
    heap: &mut [Option<Value>],
) -> bool {
    if t[arr_r] != TAG_HEAP || t[idx_r] != TAG_INT {
        return false;
    }
    let key = v[idx_r] as i64;
    match heap.get_mut(v[arr_r] as usize) {
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
    }
}

enum CallSetup<'a> {
    Enter {
        callee_idx: FunctionIndex,
        callee: &'a FastFunction,
        new_base: usize,
        callee_has_heap: bool,
    },
    EmptyDone,
    Bail,
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn call_fast_setup<'a>(
    fast_table: &'a [FastEntry],
    fast_stack: &mut FastStack,
    cur_fn_idx: FunctionIndex,
    cur_fn: &'a FastFunction,
    cur_has_heap: bool,
    base: usize,
    ip: usize,
    dst: u8,
    args_base: usize,
    count: usize,
    callee_idx: FunctionIndex,
    entry_depth: usize,
    frame_limit: usize,
    consts: &[Value],
) -> CallSetup<'a> {
    let callee: &'a FastFunction = if callee_idx == cur_fn_idx {
        cur_fn
    } else {
        match fast_table.get(callee_idx.index()) {
            Some(FastEntry::Compiled(func)) => func.as_ref(),
            _ => return CallSetup::Bail,
        }
    };

    if !callee.is_plain
        || !callee.takes_values_only
        || count != callee.param_count as usize
        || entry_depth + fast_stack.frames.len() >= frame_limit
    {
        return CallSetup::Bail;
    }

    let mut typed = callee.typed_params;
    let mut callee_has_heap = false;
    for i in 0..count {
        let at = fast_stack.tags[base + args_base + i];
        let ab = fast_stack.bits[base + args_base + i];
        if at == TAG_UNINIT {
            return CallSetup::Bail;
        }
        if at == TAG_HEAP {
            let is_ref = fast_stack
                .heap
                .get(ab as usize)
                .and_then(Option::as_ref)
                .is_none_or(Value::is_reference);
            if is_ref {
                return CallSetup::Bail;
            }
            callee_has_heap = true;
        }
        if (typed & 1) != 0 {
            let mask = callee.param_masks[i];
            if !tag_type_mask(at, ab, &fast_stack.heap, consts)
                .is_some_and(|own| mask.intersects(own))
            {
                return CallSetup::Bail;
            }
        }
        typed >>= 1;
    }

    if callee.is_empty_return_null
        && callee
            .return_mask
            .is_none_or(|m| return_mask_fits(m, TAG_NULL, 0, &fast_stack.heap, consts))
    {
        if fast_stack.tags[base + dst as usize] == TAG_HEAP
            && !release_dst_slot(
                fast_stack.bits[base + dst as usize] as u32,
                dst as usize,
                cur_fn.local_count,
                &mut fast_stack.heap,
                &mut fast_stack.free_heap,
            )
        {
            return CallSetup::Bail;
        }
        for i in 0..count {
            if fast_stack.tags[base + args_base + i] == TAG_HEAP {
                let idx = fast_stack.bits[base + args_base + i] as u32;
                fast_stack.free_heap_slot(idx);
            }
            fast_stack.tags[base + args_base + i] = TAG_UNINIT;
        }
        fast_stack.tags[base + dst as usize] = TAG_NULL;
        fast_stack.bits[base + dst as usize] = 0;
        return CallSetup::EmptyDone;
    }

    let new_base = base + cur_fn.reg_count as usize;
    fast_stack.ensure_window(new_base);

    let mut arg_tags = [TAG_UNINIT; 16];
    let mut arg_bits = [0u64; 16];
    for i in 0..count {
        arg_tags[i] = fast_stack.tags[base + args_base + i];
        arg_bits[i] = fast_stack.bits[base + args_base + i];
        fast_stack.tags[base + args_base + i] = TAG_UNINIT;
    }

    fast_stack.frames.push(FastFrame {
        function: cur_fn_idx,
        base: base as u32,
        reg_count: cur_fn.reg_count,
        has_heap: cur_has_heap,
        return_dst: dst,
        return_fast_ip: (ip + 1) as u16,
        return_bc_ip: cur_fn.bc_ip[ip] + 1,
    });

    fast_stack.tags[new_base..new_base + count].copy_from_slice(&arg_tags[..count]);
    fast_stack.bits[new_base..new_base + count].copy_from_slice(&arg_bits[..count]);
    let callee_regs = callee.reg_count as usize;
    if callee_regs > count {
        fast_stack.tags[new_base + count..new_base + callee_regs].fill(TAG_UNINIT);
    }

    CallSetup::Enter {
        callee_idx,
        callee,
        new_base,
        callee_has_heap,
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn call_noop_slow(
    t: &mut [u8; 256],
    v: &mut [u64; 256],
    dst: usize,
    args_base: usize,
    count: usize,
    local_count: u8,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
) -> bool {
    for i in 0..count {
        let at = t[args_base + i];
        if at == TAG_UNINIT {
            return false;
        }
        if at == TAG_HEAP
            && heap
                .get(v[args_base + i] as usize)
                .and_then(Option::as_ref)
                .is_none_or(Value::is_reference)
        {
            return false;
        }
    }
    if t[dst] == TAG_HEAP && !release_dst_slot(v[dst] as u32, dst, local_count, heap, free_heap) {
        return false;
    }
    for i in 0..count {
        if t[args_base + i] == TAG_HEAP
            && let Some(slot) = heap.get_mut(v[args_base + i] as usize)
        {
            *slot = None;
            free_heap.push(v[args_base + i] as u32);
        }
        t[args_base + i] = TAG_UNINIT;
    }
    t[dst] = TAG_NULL;
    v[dst] = 0;
    true
}

/// Run the Tier-2 compact VM starting from `position` inside `entry_fast`.
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

    // Ultra-fast direct entry for pure-`i64` self-recursion (`fibo_r`, `Ack`)!
    if entry_fast_ip == 0
        && entry_can_fast_return
        && let Some(ref int_rec) = entry_fast.int_rec
        && let Some(exit) = run_int_rec_entry(
            int_rec,
            entry_frame.function,
            stack,
            registers,
            &mut fast_stack.rec_regs,
            &mut fast_stack.rec_frames,
            entry_frame,
            frame_limit.saturating_sub(entry_depth),
            YIELD_AFTER_STEPS.saturating_sub(*work_io).max(1),
            work_io,
            executed_io,
        )
    {
        return exit;
    }

    let unit_consts: &[Value] = &code.cache(entry_frame.function).constants;

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
    let work_start: usize = work;
    let max_fast_frames = frame_limit.saturating_sub(entry_depth);
    let mut yield_now = false;
    let mut structural_deopt = false;

    'frame_loop: loop {
        fast_stack.ensure_window(base);
        let t: &mut [u8; 256] = (&mut fast_stack.tags[base..base + 256])
            .try_into()
            .expect("256-tag window");
        let v: &mut [u64; 256] = (&mut fast_stack.bits[base..base + 256])
            .try_into()
            .expect("256-bit window");
        let heap = &mut fast_stack.heap;
        let free_heap = &mut fast_stack.free_heap;

        loop {
            let Some(&op) = cur_ops.get(ip) else {
                break 'frame_loop;
            };

            match op.op {
                // =============================================================
                // SPECIALIZED UNBOXED INNER SCALAR LOOP KERNEL!
                // =============================================================
                Op::EnterScalarLoop => {
                    let sl = &cur_fn.scalar_loops[op.imm as usize];
                    let budget = YIELD_AFTER_STEPS.saturating_sub(work).max(1);
                    if let Some(exit) = try_run_scalar_loop(sl, t, v, budget) {
                        match exit {
                            ScalarExit::Exit {
                                next_fast_ip,
                                spent,
                            } => {
                                work += spent;
                                ip = if next_fast_ip == usize::MAX {
                                    sl.fallthrough_fast_ip as usize
                                } else {
                                    next_fast_ip
                                };
                                continue;
                            }
                            ScalarExit::Yield => {
                                work = 0;
                                yield_now = true;
                                break 'frame_loop;
                            }
                            ScalarExit::Fallback {
                                fallback_fast_ip,
                                spent,
                            } => {
                                work += spent;
                                ip = fallback_fast_ip;
                                continue;
                            }
                        }
                    }
                    // Entry guard did not match (e.g. iter 0 has TAG_INT for a
                    // float reg): fall through into the tagged ops at `ip + 1`!
                    ip += 1;
                }

                // =============================================================
                // FUSED ROTATED-LOOP LATCHES
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
                                    yield_now = true;
                                    break 'frame_loop;
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
                                    yield_now = true;
                                    break 'frame_loop;
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
                                    yield_now = true;
                                    break 'frame_loop;
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
                                    yield_now = true;
                                    break 'frame_loop;
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
                                    yield_now = true;
                                    break 'frame_loop;
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
                                    yield_now = true;
                                    break 'frame_loop;
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
                                    yield_now = true;
                                    break 'frame_loop;
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
                                    yield_now = true;
                                    break 'frame_loop;
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
                // FLOAT-FIRST ARITHMETIC
                // =============================================================
                Op::AddF64 => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (rt, rb) = (t[op.c as usize], v[op.c as usize]);
                    let (dt, db) = if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(lb) + f64::from_bits(rb)).to_bits(),
                        )
                    } else if let Some(pair) = num_binary_slow(NUM_ADD, lt, lb, rt, rb) {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubF64 => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (rt, rb) = (t[op.c as usize], v[op.c as usize]);
                    let (dt, db) = if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(lb) - f64::from_bits(rb)).to_bits(),
                        )
                    } else if let Some(pair) = num_binary_slow(NUM_SUB, lt, lb, rt, rb) {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulF64 => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (rt, rb) = (t[op.c as usize], v[op.c as usize]);
                    let (dt, db) = if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        (
                            TAG_FLOAT,
                            (f64::from_bits(lb) * f64::from_bits(rb)).to_bits(),
                        )
                    } else if let Some(pair) = num_binary_slow(NUM_MUL, lt, lb, rt, rb) {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::DivF64 => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (rt, rb) = (t[op.c as usize], v[op.c as usize]);
                    let (dt, db) = if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        let y = f64::from_bits(rb);
                        if y == 0.0 {
                            break 'frame_loop;
                        }
                        (TAG_FLOAT, (f64::from_bits(lb) / y).to_bits())
                    } else if let Some(pair) = num_binary_slow(NUM_DIV, lt, lb, rt, rb) {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulF64Imm => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (dt, db) = if lt == TAG_FLOAT {
                        (TAG_FLOAT, (f64::from_bits(lb) * (op.imm as f64)).to_bits())
                    } else if let Some(pair) =
                        num_binary_slow(NUM_MUL, lt, lb, TAG_INT, i64::from(op.imm) as u64)
                    {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::AddF64Imm => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (dt, db) = if lt == TAG_FLOAT {
                        (TAG_FLOAT, (f64::from_bits(lb) + (op.imm as f64)).to_bits())
                    } else if let Some(pair) =
                        num_binary_slow(NUM_ADD, lt, lb, TAG_INT, i64::from(op.imm) as u64)
                    {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubF64Imm => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (dt, db) = if lt == TAG_FLOAT {
                        (TAG_FLOAT, (f64::from_bits(lb) - (op.imm as f64)).to_bits())
                    } else if let Some(pair) =
                        num_binary_slow(NUM_SUB, lt, lb, TAG_INT, i64::from(op.imm) as u64)
                    {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // INT-FIRST ARITHMETIC
                // =============================================================
                Op::AddNum => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (rt, rb) = (t[op.c as usize], v[op.c as usize]);
                    let (dt, db) = if lt == TAG_INT && rt == TAG_INT {
                        match (lb as i64).checked_add(rb as i64) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (
                                TAG_FLOAT,
                                (((lb as i64) as f64) + ((rb as i64) as f64)).to_bits(),
                            ),
                        }
                    } else if let Some(pair) = num_binary_slow(NUM_ADD, lt, lb, rt, rb) {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubNum => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (rt, rb) = (t[op.c as usize], v[op.c as usize]);
                    let (dt, db) = if lt == TAG_INT && rt == TAG_INT {
                        match (lb as i64).checked_sub(rb as i64) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (
                                TAG_FLOAT,
                                (((lb as i64) as f64) - ((rb as i64) as f64)).to_bits(),
                            ),
                        }
                    } else if let Some(pair) = num_binary_slow(NUM_SUB, lt, lb, rt, rb) {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulNum => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (rt, rb) = (t[op.c as usize], v[op.c as usize]);
                    let (dt, db) = if lt == TAG_INT && rt == TAG_INT {
                        match (lb as i64).checked_mul(rb as i64) {
                            Some(p) => (TAG_INT, p as u64),
                            None => (
                                TAG_FLOAT,
                                (((lb as i64) as f64) * ((rb as i64) as f64)).to_bits(),
                            ),
                        }
                    } else if let Some(pair) = num_binary_slow(NUM_MUL, lt, lb, rt, rb) {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::DivNum => {
                    let Some((dt, db)) = num_binary_slow(
                        NUM_DIV,
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
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ModInt => {
                    if t[op.b as usize] != TAG_INT || t[op.c as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let y = v[op.c as usize] as i64;
                    if y == 0 {
                        break 'frame_loop;
                    }
                    let res = (v[op.b as usize] as i64).wrapping_rem(y);
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitAnd => {
                    if t[op.b as usize] != TAG_INT || t[op.c as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let res = v[op.b as usize] & v[op.c as usize];
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitOr => {
                    if t[op.b as usize] != TAG_INT || t[op.c as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let res = v[op.b as usize] | v[op.c as usize];
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitXor => {
                    if t[op.b as usize] != TAG_INT || t[op.c as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let res = v[op.b as usize] ^ v[op.c as usize];
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Shl => {
                    if t[op.b as usize] != TAG_INT || t[op.c as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let s = v[op.c as usize] as i64;
                    if s < 0 {
                        break 'frame_loop;
                    }
                    let res = if s >= 64 {
                        0
                    } else {
                        (v[op.b as usize] as i64).wrapping_shl(s as u32)
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Shr => {
                    if t[op.b as usize] != TAG_INT || t[op.c as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let s = v[op.c as usize] as i64;
                    if s < 0 {
                        break 'frame_loop;
                    }
                    let a = v[op.b as usize] as i64;
                    let res = if s >= 64 { a >> 63 } else { a >> (s as u32) };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // IMMEDIATE ARITHMETIC
                // =============================================================
                Op::AddImm => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (dt, db) = if lt == TAG_INT {
                        match (lb as i64).checked_add(i64::from(op.imm)) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (
                                TAG_FLOAT,
                                (((lb as i64) as f64) + (op.imm as f64)).to_bits(),
                            ),
                        }
                    } else if let Some(pair) =
                        num_binary_slow(NUM_ADD, lt, lb, TAG_INT, i64::from(op.imm) as u64)
                    {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubImm => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (dt, db) = if lt == TAG_INT {
                        match (lb as i64).checked_sub(i64::from(op.imm)) {
                            Some(s) => (TAG_INT, s as u64),
                            None => (
                                TAG_FLOAT,
                                (((lb as i64) as f64) - (op.imm as f64)).to_bits(),
                            ),
                        }
                    } else if let Some(pair) =
                        num_binary_slow(NUM_SUB, lt, lb, TAG_INT, i64::from(op.imm) as u64)
                    {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulImm => {
                    let (lt, lb) = (t[op.b as usize], v[op.b as usize]);
                    let (dt, db) = if lt == TAG_INT {
                        match (lb as i64).checked_mul(i64::from(op.imm)) {
                            Some(p) => (TAG_INT, p as u64),
                            None => (
                                TAG_FLOAT,
                                (((lb as i64) as f64) * (op.imm as f64)).to_bits(),
                            ),
                        }
                    } else if let Some(pair) =
                        num_binary_slow(NUM_MUL, lt, lb, TAG_INT, i64::from(op.imm) as u64)
                    {
                        pair
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ModImm => {
                    if t[op.b as usize] != TAG_INT || op.imm == 0 {
                        break 'frame_loop;
                    }
                    let res = (v[op.b as usize] as i64).wrapping_rem(i64::from(op.imm));
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitAndImm => {
                    if t[op.b as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let res = (v[op.b as usize] as i64) & i64::from(op.imm);
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitOrImm => {
                    if t[op.b as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let res = (v[op.b as usize] as i64) | i64::from(op.imm);
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitXorImm => {
                    if t[op.b as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    let res = (v[op.b as usize] as i64) ^ i64::from(op.imm);
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ShlImm => {
                    if t[op.b as usize] != TAG_INT || op.imm < 0 {
                        break 'frame_loop;
                    }
                    let res = if op.imm >= 64 {
                        0
                    } else {
                        (v[op.b as usize] as i64).wrapping_shl(op.imm as u32)
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ShrImm => {
                    if t[op.b as usize] != TAG_INT || op.imm < 0 {
                        break 'frame_loop;
                    }
                    let a = v[op.b as usize] as i64;
                    let res = if op.imm >= 64 {
                        a >> 63
                    } else {
                        a >> (op.imm as u32)
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // COMPOUND LOCAL UPDATES
                // =============================================================
                Op::AddAssign => {
                    let loc = op.a as usize;
                    let src = op.b as usize;
                    let (lt, rt) = (t[loc], t[src]);
                    if lt == TAG_INT && rt == TAG_INT {
                        match (v[loc] as i64).checked_add(v[src] as i64) {
                            Some(s) => v[loc] = s as u64,
                            None => {
                                t[loc] = TAG_FLOAT;
                                v[loc] =
                                    (((v[loc] as i64) as f64) + ((v[src] as i64) as f64)).to_bits();
                            }
                        }
                    } else if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        v[loc] = (f64::from_bits(v[loc]) + f64::from_bits(v[src])).to_bits();
                    } else if let Some((dt, db)) = num_binary_slow(NUM_ADD, lt, v[loc], rt, v[src])
                    {
                        t[loc] = dt;
                        v[loc] = db;
                    } else {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubAssign => {
                    let loc = op.a as usize;
                    let src = op.b as usize;
                    let (lt, rt) = (t[loc], t[src]);
                    if lt == TAG_INT && rt == TAG_INT {
                        match (v[loc] as i64).checked_sub(v[src] as i64) {
                            Some(s) => v[loc] = s as u64,
                            None => {
                                t[loc] = TAG_FLOAT;
                                v[loc] =
                                    (((v[loc] as i64) as f64) - ((v[src] as i64) as f64)).to_bits();
                            }
                        }
                    } else if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        v[loc] = (f64::from_bits(v[loc]) - f64::from_bits(v[src])).to_bits();
                    } else if let Some((dt, db)) = num_binary_slow(NUM_SUB, lt, v[loc], rt, v[src])
                    {
                        t[loc] = dt;
                        v[loc] = db;
                    } else {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulAssign => {
                    let loc = op.a as usize;
                    let src = op.b as usize;
                    let (lt, rt) = (t[loc], t[src]);
                    if lt == TAG_INT && rt == TAG_INT {
                        match (v[loc] as i64).checked_mul(v[src] as i64) {
                            Some(p) => v[loc] = p as u64,
                            None => {
                                t[loc] = TAG_FLOAT;
                                v[loc] =
                                    (((v[loc] as i64) as f64) * ((v[src] as i64) as f64)).to_bits();
                            }
                        }
                    } else if lt == TAG_FLOAT && rt == TAG_FLOAT {
                        v[loc] = (f64::from_bits(v[loc]) * f64::from_bits(v[src])).to_bits();
                    } else if let Some((dt, db)) = num_binary_slow(NUM_MUL, lt, v[loc], rt, v[src])
                    {
                        t[loc] = dt;
                        v[loc] = db;
                    } else {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulAddAssign => {
                    let acc = op.a as usize;
                    let lhs = op.b as usize;
                    let rhs = op.c as usize;
                    if t[acc] == TAG_INT && t[lhs] == TAG_INT && t[rhs] == TAG_INT {
                        let Some(prod) = (v[lhs] as i64).checked_mul(v[rhs] as i64) else {
                            break 'frame_loop;
                        };
                        let Some(sum) = (v[acc] as i64).checked_add(prod) else {
                            break 'frame_loop;
                        };
                        v[acc] = sum as u64;
                        ip += 1;
                    } else if t[acc] == TAG_FLOAT && t[lhs] == TAG_FLOAT && t[rhs] == TAG_FLOAT {
                        let res = f64::from_bits(v[acc])
                            + f64::from_bits(v[lhs]) * f64::from_bits(v[rhs]);
                        v[acc] = res.to_bits();
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::StrAppend => {
                    if !str_append_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        &mut cur_has_heap,
                        heap,
                        free_heap,
                        unit_consts,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // CONSTANTS, MOVES, CLEARS
                // =============================================================
                Op::LoadInt => {
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        i64::from(op.imm) as u64,
                        cur_local_count,
                        heap,
                        free_heap,
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
                        op.a as usize,
                        TAG_FLOAT,
                        bits,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadBool => {
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_BOOL,
                        u64::from(op.b),
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadNull => {
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_NULL,
                        0,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadConstRef => {
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_CONST,
                        op.imm as u32 as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::NewArray => {
                    if !new_array_helper(
                        t,
                        v,
                        op.a as usize,
                        op.imm,
                        cur_local_count,
                        &mut cur_has_heap,
                        heap,
                        free_heap,
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
                        if !write_scalar(t, v, dst, st, v[src], cur_local_count, heap, free_heap) {
                            break 'frame_loop;
                        }
                    } else if !move_heap_slow(
                        t,
                        v,
                        dst,
                        v[src],
                        cur_local_count,
                        &mut cur_has_heap,
                        heap,
                        free_heap,
                        unit_consts,
                    ) {
                        break 'frame_loop;
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
                        if !write_scalar(t, v, dst, st, v[src], cur_local_count, heap, free_heap) {
                            break 'frame_loop;
                        }
                        if dst != src {
                            t[src] = TAG_UNINIT;
                        }
                    } else if !assign_take_heap_slow(
                        t,
                        v,
                        dst,
                        src,
                        cur_local_count,
                        &mut cur_has_heap,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
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
                    let sb = v[op.b as usize];
                    let (dt, db) = if st == TAG_FLOAT {
                        (TAG_FLOAT, (-f64::from_bits(sb)).to_bits())
                    } else if st == TAG_INT {
                        match (sb as i64).checked_neg() {
                            Some(n) => (TAG_INT, n as u64),
                            None => (TAG_FLOAT, (-((sb as i64) as f64)).to_bits()),
                        }
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        dt,
                        db,
                        cur_local_count,
                        heap,
                        free_heap,
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
                        op.a as usize,
                        TAG_BOOL,
                        u64::from(!truthy),
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitNotInt => {
                    if t[op.b as usize] != TAG_INT {
                        break 'frame_loop;
                    }
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        !v[op.b as usize],
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::CastInt => {
                    let st = t[op.b as usize];
                    let sb = v[op.b as usize];
                    let res = match st {
                        TAG_INT => sb,
                        TAG_FLOAT => {
                            let f = f64::from_bits(sb);
                            if f.is_finite()
                                && (-9223372036854775808.0..9223372036854775808.0).contains(&f)
                            {
                                (f as i64) as u64
                            } else {
                                break 'frame_loop;
                            }
                        }
                        TAG_NULL => 0,
                        TAG_BOOL => u64::from(sb != 0),
                        _ => break 'frame_loop,
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        res,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::CastFloat => {
                    let st = t[op.b as usize];
                    let sb = v[op.b as usize];
                    let bits = match st {
                        TAG_FLOAT => sb,
                        TAG_INT => ((sb as i64) as f64).to_bits(),
                        TAG_NULL => 0.0f64.to_bits(),
                        TAG_BOOL => (if sb != 0 { 1.0f64 } else { 0.0f64 }).to_bits(),
                        _ => break 'frame_loop,
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_FLOAT,
                        bits,
                        cur_local_count,
                        heap,
                        free_heap,
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
                        op.a as usize,
                        TAG_BOOL,
                        u64::from(truthy),
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::StrLen => {
                    let st = t[op.b as usize];
                    let len = if st == TAG_CONST {
                        match unit_consts.get(v[op.b as usize] as usize) {
                            Some(Value::InternedStr(s)) => s.len(),
                            Some(Value::String(s)) => s.len(),
                            _ => break 'frame_loop,
                        }
                    } else if st == TAG_HEAP {
                        match heap.get(v[op.b as usize] as usize) {
                            Some(Some(Value::String(s))) => s.len(),
                            Some(Some(Value::InternedStr(s))) => s.len(),
                            _ => break 'frame_loop,
                        }
                    } else {
                        break 'frame_loop;
                    };
                    if !write_scalar(
                        t,
                        v,
                        op.a as usize,
                        TAG_INT,
                        len as u64,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::IncLocalFast => {
                    let loc = op.a as usize;
                    if t[loc] == TAG_INT {
                        let next = (v[loc] as i64).wrapping_add(1);
                        if next != i64::MIN {
                            v[loc] = next as u64;
                            ip += 1;
                            continue;
                        }
                        t[loc] = TAG_FLOAT;
                        v[loc] = ((i64::MAX as f64) + 1.0).to_bits();
                        ip += 1;
                    } else if t[loc] == TAG_FLOAT {
                        v[loc] = (f64::from_bits(v[loc]) + 1.0).to_bits();
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::DecLocalFast => {
                    let loc = op.a as usize;
                    if t[loc] == TAG_INT {
                        let next = (v[loc] as i64).wrapping_sub(1);
                        if next != i64::MAX {
                            v[loc] = next as u64;
                            ip += 1;
                            continue;
                        }
                        t[loc] = TAG_FLOAT;
                        v[loc] = ((i64::MIN as f64) - 1.0).to_bits();
                        ip += 1;
                    } else if t[loc] == TAG_FLOAT {
                        v[loc] = (f64::from_bits(v[loc]) - 1.0).to_bits();
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::IncDecLocal => {
                    let dst = op.a as usize;
                    let loc = op.b as usize;
                    let mode = op.c; // 0=PreInc, 1=PostInc, 2=PreDec, 3=PostDec
                    let old_t = t[loc];
                    let old_b = v[loc];
                    let (new_t, new_b) = if old_t == TAG_INT {
                        let cur = old_b as i64;
                        if mode < 2 {
                            match cur.checked_add(1) {
                                Some(n) => (TAG_INT, n as u64),
                                None => (TAG_FLOAT, ((cur as f64) + 1.0).to_bits()),
                            }
                        } else {
                            match cur.checked_sub(1) {
                                Some(n) => (TAG_INT, n as u64),
                                None => (TAG_FLOAT, ((cur as f64) - 1.0).to_bits()),
                            }
                        }
                    } else if old_t == TAG_FLOAT {
                        let d = if mode < 2 { 1.0 } else { -1.0 };
                        (TAG_FLOAT, (f64::from_bits(old_b) + d).to_bits())
                    } else {
                        break 'frame_loop;
                    };
                    let (rt, rb) = if (mode & 1) == 0 {
                        (new_t, new_b)
                    } else {
                        (old_t, old_b)
                    };
                    if !write_scalar(t, v, dst, rt, rb, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    t[loc] = new_t;
                    v[loc] = new_b;
                    ip += 1;
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
                        op.a as usize,
                        TAG_BOOL,
                        u64::from(res),
                        cur_local_count,
                        heap,
                        free_heap,
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
                            ip = target;
                            yield_now = true;
                            break 'frame_loop;
                        }
                    }
                    ip = target;
                }
                Op::JumpIf | Op::JumpIfNot => {
                    let Some(truthy) = scalar_truthy(t[op.a as usize], v[op.a as usize]) else {
                        break 'frame_loop;
                    };
                    let take = if op.op == Op::JumpIf { truthy } else { !truthy };
                    if take {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                ip = target;
                                yield_now = true;
                                break 'frame_loop;
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }
                Op::JumpIfNull | Op::JumpIfNotNull => {
                    let st = t[op.a as usize];
                    if st == TAG_UNINIT || st == TAG_HEAP {
                        break 'frame_loop;
                    }
                    let is_null = st == TAG_NULL;
                    let take = if op.op == Op::JumpIfNull {
                        is_null
                    } else {
                        !is_null
                    };
                    if take {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                ip = target;
                                yield_now = true;
                                break 'frame_loop;
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
                                ip = target;
                                yield_now = true;
                                break 'frame_loop;
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
                        && lt != TAG_HEAP
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
                                ip = target;
                                yield_now = true;
                                break 'frame_loop;
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }

                // =============================================================
                // IN-LOOP ARRAY OPS (DIRECT LIST SLICE + HOISTED INNER LOOPS!)
                // =============================================================
                Op::ArrGetInt => {
                    if !arr_get_int_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        op.imm != 0,
                        cur_local_count,
                        &mut cur_has_heap,
                        heap,
                        free_heap,
                        unit_consts,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Arr2DGetInt => {
                    if !arr_2d_get_int_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        (op.imm & 0xff) as usize,
                        (op.imm >> 8) != 0,
                        cur_local_count,
                        &mut cur_has_heap,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrSetInt => {
                    if !arr_set_int_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        &mut cur_has_heap,
                        heap,
                        free_heap,
                        unit_consts,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Arr2DSetInt => {
                    if !arr_2d_set_int_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        op.imm as usize,
                        &mut cur_has_heap,
                        heap,
                        free_heap,
                        unit_consts,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrAddAssignInt => {
                    if !arr_add_assign_int_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrAddFromArr => {
                    if !arr_add_from_arr_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrAddFromArrLoop => {
                    let budget = YIELD_AFTER_STEPS.saturating_sub(work).max(1);
                    match arr_add_from_arr_loop_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        i64::from(op.imm),
                        heap,
                        budget,
                    ) {
                        ArrayLoopExit::Done(spent) => {
                            work += spent;
                            ip += 1;
                        }
                        ArrayLoopExit::Yield => {
                            work = 0;
                            yield_now = true;
                            break 'frame_loop;
                        }
                        ArrayLoopExit::Bail(spent) => {
                            work += spent;
                            break 'frame_loop;
                        }
                    }
                }
                Op::ArrUnsetStepLoop => {
                    let budget = YIELD_AFTER_STEPS.saturating_sub(work).max(1);
                    match arr_unset_step_loop_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        i64::from(op.imm),
                        heap,
                        budget,
                    ) {
                        ArrayLoopExit::Done(spent) => {
                            work += spent;
                            ip += 1;
                        }
                        ArrayLoopExit::Yield => {
                            work = 0;
                            yield_now = true;
                            break 'frame_loop;
                        }
                        ArrayLoopExit::Bail(spent) => {
                            work += spent;
                            break 'frame_loop;
                        }
                    }
                }
                Op::ArrSetRegStepLoop => {
                    let budget = YIELD_AFTER_STEPS.saturating_sub(work).max(1);
                    match arr_set_reg_step_loop_helper(
                        t,
                        v,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        op.imm,
                        heap,
                        unit_consts,
                        budget,
                    ) {
                        ArrayLoopExit::Done(spent) => {
                            work += spent;
                            ip += 1;
                        }
                        ArrayLoopExit::Yield => {
                            work = 0;
                            yield_now = true;
                            break 'frame_loop;
                        }
                        ArrayLoopExit::Bail(spent) => {
                            work += spent;
                            break 'frame_loop;
                        }
                    }
                }
                Op::ArrPush => {
                    if !arr_push_helper(t, v, op.a as usize, op.b as usize, heap, unit_consts) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrUnsetInt => {
                    if !arr_unset_int_helper(t, v, op.a as usize, op.b as usize, heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // SPECIALIZED CALLS & RETURNS
                // =============================================================
                Op::CallNoop => {
                    let dst = op.a as usize;
                    let args_base = op.b as usize;
                    let count = op.c as usize;
                    if count == 1
                        && t[args_base] != TAG_UNINIT
                        && t[args_base] != TAG_HEAP
                        && t[dst] != TAG_HEAP
                    {
                        t[args_base] = TAG_UNINIT;
                        t[dst] = TAG_NULL;
                        v[dst] = 0;
                    } else if count == 0 && t[dst] != TAG_HEAP {
                        t[dst] = TAG_NULL;
                        v[dst] = 0;
                    } else if !call_noop_slow(
                        t,
                        v,
                        dst,
                        args_base,
                        count,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::CallSelf1 => {
                    let arg_r = op.b as usize;
                    let at = t[arg_r];
                    if at != TAG_UNINIT
                        && at != TAG_HEAP
                        && fast_stack.frames.len() < max_fast_frames
                    {
                        let ab = v[arg_r];
                        t[arg_r] = TAG_UNINIT;
                        let new_base = base + cur_reg_count as usize;
                        fast_stack.frames.push(FastFrame {
                            function: cur_fn_idx,
                            base: base as u32,
                            reg_count: cur_reg_count,
                            has_heap: cur_has_heap,
                            return_dst: op.a,
                            return_fast_ip: (ip + 1) as u16,
                            return_bc_ip: cur_fn.bc_ip[ip] + 1,
                        });
                        fast_stack.ensure_window(new_base);
                        fast_stack.tags[new_base] = at;
                        fast_stack.bits[new_base] = ab;
                        let regs_n = cur_reg_count as usize;
                        if regs_n > 1 {
                            fast_stack.tags[new_base + 1..new_base + regs_n].fill(TAG_UNINIT);
                        }
                        cur_has_heap = false;
                        base = new_base;
                        ip = 0;
                        work += 1;
                        if work >= YIELD_AFTER_STEPS {
                            work = 0;
                            yield_now = true;
                            break 'frame_loop;
                        }
                        continue 'frame_loop;
                    }
                    match call_fast_setup(
                        fast_table,
                        fast_stack,
                        cur_fn_idx,
                        cur_fn,
                        cur_has_heap,
                        base,
                        ip,
                        op.a,
                        op.b as usize,
                        1,
                        cur_fn_idx,
                        entry_depth,
                        frame_limit,
                        unit_consts,
                    ) {
                        CallSetup::Enter {
                            new_base,
                            callee_has_heap,
                            ..
                        } => {
                            cur_has_heap = callee_has_heap;
                            base = new_base;
                            ip = 0;
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                yield_now = true;
                                break 'frame_loop;
                            }
                            continue 'frame_loop;
                        }
                        CallSetup::EmptyDone => {
                            ip += 1;
                            continue 'frame_loop;
                        }
                        CallSetup::Bail => break 'frame_loop,
                    }
                }
                Op::CallSelf2 => {
                    let a0 = op.b as usize;
                    let a1 = op.c as usize;
                    let (t0, t1) = (t[a0], t[a1]);
                    if t0 != TAG_UNINIT
                        && t0 != TAG_HEAP
                        && t1 != TAG_UNINIT
                        && t1 != TAG_HEAP
                        && fast_stack.frames.len() < max_fast_frames
                    {
                        let (b0, b1) = (v[a0], v[a1]);
                        t[a0] = TAG_UNINIT;
                        t[a1] = TAG_UNINIT;
                        let new_base = base + cur_reg_count as usize;
                        fast_stack.frames.push(FastFrame {
                            function: cur_fn_idx,
                            base: base as u32,
                            reg_count: cur_reg_count,
                            has_heap: cur_has_heap,
                            return_dst: op.a,
                            return_fast_ip: (ip + 1) as u16,
                            return_bc_ip: cur_fn.bc_ip[ip] + 1,
                        });
                        fast_stack.ensure_window(new_base);
                        fast_stack.tags[new_base] = t0;
                        fast_stack.bits[new_base] = b0;
                        fast_stack.tags[new_base + 1] = t1;
                        fast_stack.bits[new_base + 1] = b1;
                        let regs_n = cur_reg_count as usize;
                        if regs_n > 2 {
                            fast_stack.tags[new_base + 2..new_base + regs_n].fill(TAG_UNINIT);
                        }
                        cur_has_heap = false;
                        base = new_base;
                        ip = 0;
                        work += 1;
                        if work >= YIELD_AFTER_STEPS {
                            work = 0;
                            yield_now = true;
                            break 'frame_loop;
                        }
                        continue 'frame_loop;
                    }
                    match call_fast_setup(
                        fast_table,
                        fast_stack,
                        cur_fn_idx,
                        cur_fn,
                        cur_has_heap,
                        base,
                        ip,
                        op.a,
                        op.b as usize,
                        2,
                        cur_fn_idx,
                        entry_depth,
                        frame_limit,
                        unit_consts,
                    ) {
                        CallSetup::Enter {
                            new_base,
                            callee_has_heap,
                            ..
                        } => {
                            cur_has_heap = callee_has_heap;
                            base = new_base;
                            ip = 0;
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                yield_now = true;
                                break 'frame_loop;
                            }
                            continue 'frame_loop;
                        }
                        CallSetup::EmptyDone => {
                            ip += 1;
                            continue 'frame_loop;
                        }
                        CallSetup::Bail => break 'frame_loop,
                    }
                }
                Op::CallFast => {
                    let callee_idx = FunctionIndex(op.imm as u32);
                    match call_fast_setup(
                        fast_table,
                        fast_stack,
                        cur_fn_idx,
                        cur_fn,
                        cur_has_heap,
                        base,
                        ip,
                        op.a,
                        op.b as usize,
                        op.c as usize,
                        callee_idx,
                        entry_depth,
                        frame_limit,
                        unit_consts,
                    ) {
                        CallSetup::Enter {
                            callee_idx,
                            callee,
                            new_base,
                            callee_has_heap,
                        } => {
                            if callee_idx != cur_fn_idx {
                                cur_fn_idx = callee_idx;
                                cur_fn = callee;
                                cur_ops = &callee.ops[..];
                                cur_f64 = &callee.f64_consts[..];
                                cur_local_count = callee.local_count;
                                cur_reg_count = callee.reg_count;
                            }
                            cur_has_heap = callee_has_heap;
                            base = new_base;
                            ip = 0;
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                yield_now = true;
                                break 'frame_loop;
                            }
                            continue 'frame_loop;
                        }
                        CallSetup::EmptyDone => {
                            ip += 1;
                            continue 'frame_loop;
                        }
                        CallSetup::Bail => break 'frame_loop,
                    }
                }
                Op::RetIntImm | Op::ReturnReg | Op::ReturnNull => {
                    let (ret_t, ret_b) = match op.op {
                        Op::RetIntImm => (TAG_INT, i64::from(op.imm) as u64),
                        Op::ReturnNull => (TAG_NULL, 0),
                        Op::ReturnReg => {
                            let rt = t[op.a as usize];
                            if rt == TAG_UNINIT {
                                break 'frame_loop;
                            }
                            let rb = v[op.a as usize];
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
                        && !return_mask_fits(mask, ret_t, ret_b, heap, unit_consts)
                    {
                        break 'frame_loop;
                    }

                    if let Some(caller_fast) = fast_stack.frames.pop() {
                        if ret_t == TAG_HEAP && op.op == Op::ReturnReg {
                            t[op.a as usize] = TAG_UNINIT;
                        }
                        if cur_has_heap {
                            fast_stack.release_frame_heap(base, cur_reg_count as usize);
                        }
                        let caller_base = caller_fast.base as usize;
                        let caller_dst = caller_fast.return_dst as usize;
                        let mut caller_has_heap = caller_fast.has_heap;

                        if fast_stack.tags[caller_base + caller_dst] == TAG_HEAP {
                            let old_idx = fast_stack.bits[caller_base + caller_dst] as u32;
                            fast_stack.free_heap_slot(old_idx);
                        }
                        fast_stack.tags[caller_base + caller_dst] = ret_t;
                        fast_stack.bits[caller_base + caller_dst] = ret_b;
                        if ret_t == TAG_HEAP {
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
                        structural_deopt = true;
                        break 'frame_loop;
                    }
                    let Some(caller_vm) = stack.caller().copied() else {
                        structural_deopt = true;
                        break 'frame_loop;
                    };
                    let Some(destination) = entry_frame.destination() else {
                        structural_deopt = true;
                        break 'frame_loop;
                    };
                    if ret_t == TAG_HEAP && op.op == Op::ReturnReg {
                        t[op.a as usize] = TAG_UNINIT;
                    }
                    let Some(ret_value) = fast_stack.take_value(ret_t, ret_b, unit_consts) else {
                        break 'frame_loop;
                    };
                    fast_stack.reset();
                    stack.pop_for_return();
                    registers.truncate(entry_frame.start());
                    registers.put(caller_vm.base() + destination.index(), ret_value);
                    *work_io = work;
                    *executed_io += (work.saturating_sub(work_start) as u64).max(1);
                    return FastExit::Returned(Position {
                        frame: caller_vm,
                        ip: caller_vm.ip(),
                    });
                }

                // =============================================================
                // STRUCTURAL DEOPT (e.g. `echo` in `mandel2` — NOT a type deopt!)
                // =============================================================
                Op::Deopt => {
                    structural_deopt = true;
                    break 'frame_loop;
                }
            }
        }
    }

    // Track runtime type-deopts (ignore yields and structural `Op::Deopt`).
    // After 2 runtime type-deopts on the same function, disable Tier 2 for it
    // so `SplFixedArray` / string-keyed-map scripts never thrash!
    let disable_fn = if !yield_now && !structural_deopt && fast_stack.frames.is_empty() {
        let prev = cur_fn.type_deopts.fetch_add(1, Ordering::Relaxed);
        (prev >= 1).then_some(cur_fn_idx)
    } else {
        None
    };

    materialize_deopt(
        code,
        stack,
        registers,
        fast_stack,
        entry_frame,
        entry_fast.reg_count as usize,
        cur_fn_idx,
        cur_fn,
        base,
        ip,
        yield_now,
        disable_fn,
        work_io,
        executed_io,
        work,
        (work.saturating_sub(work_start) as u64).max(1),
    )
}

#[cold]
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn materialize_deopt(
    code: &CodeTable,
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
    disable_fn: Option<FunctionIndex>,
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

    let entry_consts = &code.cache(entry_frame.function).constants;
    fast_stack.store_frame_to_vm(
        registers,
        entry_consts,
        0,
        entry_frame.base(),
        entry_reg_count,
    );

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
            let consts = &code.cache(f.function).constants;
            fast_stack.store_frame_to_vm(
                registers,
                consts,
                f.base as usize,
                vm_base,
                f.reg_count as usize,
            );
            let mut vm_frame = Frame::new(f.function, vm_base, Some(Reg(u16::from(prev_dst))));
            vm_frame.set_ip(f.return_bc_ip as usize);
            stack.push(vm_frame);
        }
        let last_dst = saved.last().expect("non-empty saved frames").return_dst;
        let inner_vm_base = registers.len();
        registers.grow_to(inner_vm_base + cur_fn.reg_count as usize);
        let cur_consts = &code.cache(cur_fn_idx).constants;
        fast_stack.store_frame_to_vm(
            registers,
            cur_consts,
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
        disable_fn,
    }
}
