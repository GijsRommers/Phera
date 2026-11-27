//! Tier-2 compact register VM execution engine (100% safe Rust).
//!
//! Runs 8-byte [`FastOp`] streams over a single `&mut [u64; 256]` NaN-boxed
//! register window (zero bounds checks by type). All heap, array, string,
//! mixed-type slow arithmetic, general call, and deoptimization paths are
//! placed in `#[inline(never)]` helpers so the hot dispatch loop stays small
//! and spill-free.

use std::rc::Rc;

use phera_bytecode::{Reg, TypeMask};
use phera_value::{PhpArray, PhpString, Value};

use crate::calls::call_stack::CallStack;
use crate::calls::execution_loop::{Position, YIELD_AFTER_STEPS};
use crate::calls::frame::Frame;
use crate::calls::prepared_call::CallKind;
use crate::calls::registers::Registers;
use crate::code_table::{CodeTable, FunctionIndex};
use crate::fast_tier::stack::{FastFrame, FastStack};
use crate::fast_tier::{
    CMP_EQ, CMP_GE, CMP_GT, CMP_IDENT, CMP_LE, CMP_LT, CMP_NE, CMP_NOT_IDENT, FastEntry, FastExit,
    FastFunction, Op, PAYLOAD_MASK, TAG_BOOL, TAG_CONST, TAG_HEAP, TAG_INT, TAG_MASK, TAG_NULL,
    TAG_UNINIT, box_bool, box_f64, box_i32, box_int_checked, is_f64, is_int, unbox_f64, unbox_int,
};

const NUM_ADD: u8 = 0;
const NUM_SUB: u8 = 1;
const NUM_MUL: u8 = 2;
const NUM_DIV: u8 = 3;

#[inline(always)]
fn scalar_truthy(w: u64) -> Option<bool> {
    if is_f64(w) {
        return Some(unbox_f64(w) != 0.0);
    }
    match w & TAG_MASK {
        TAG_NULL => Some(false),
        TAG_BOOL => Some((w & 1) != 0),
        TAG_INT => Some((w & PAYLOAD_MASK) != 0),
        _ => None,
    }
}

#[inline(always)]
fn word_type_mask(w: u64, heap: &[Option<Value>], consts: &[Value]) -> Option<TypeMask> {
    if is_f64(w) {
        return Some(TypeMask::FLOAT);
    }
    match w & TAG_MASK {
        TAG_NULL => Some(TypeMask::NULL),
        TAG_BOOL => Some(if (w & 1) != 0 {
            TypeMask::TRUE
        } else {
            TypeMask::FALSE
        }),
        TAG_INT => Some(TypeMask::INT),
        TAG_CONST => {
            let val = consts.get(w as u32 as usize)?;
            Some(crate::types::own_type(val))
        }
        TAG_HEAP => {
            let val = heap.get(w as u32 as usize)?.as_ref()?;
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
fn return_mask_fits(mask: TypeMask, w: u64, heap: &[Option<Value>], consts: &[Value]) -> bool {
    if w == TAG_NULL && mask.intersects(TypeMask::VOID) {
        return true;
    }
    word_type_mask(w, heap, consts).is_some_and(|own| mask.intersects(own))
}

#[inline(always)]
fn compare_words(cmp: u8, a: u64, b: u64) -> Option<bool> {
    if is_int(a) && is_int(b) {
        let x = unbox_int(a);
        let y = unbox_int(b);
        return Some(match cmp {
            CMP_EQ | CMP_IDENT => x == y,
            CMP_NE | CMP_NOT_IDENT => x != y,
            CMP_LT => x < y,
            CMP_LE => x <= y,
            CMP_GT => x > y,
            CMP_GE => x >= y,
            _ => return None,
        });
    }
    if is_f64(a) && is_f64(b) {
        let x = unbox_f64(a);
        let y = unbox_f64(b);
        return Some(match cmp {
            CMP_EQ | CMP_IDENT => x == y,
            CMP_NE | CMP_NOT_IDENT => x != y,
            CMP_LT => x < y,
            CMP_LE => x <= y,
            CMP_GT => x > y,
            CMP_GE => x >= y,
            _ => return None,
        });
    }
    compare_words_slow(cmp, a, b)
}

#[inline(never)]
fn compare_words_slow(cmp: u8, a: u64, b: u64) -> Option<bool> {
    let a_num = is_f64(a) || is_int(a);
    let b_num = is_f64(b) || is_int(b);
    if a_num && b_num {
        if cmp == CMP_IDENT {
            return Some(false);
        }
        if cmp == CMP_NOT_IDENT {
            return Some(true);
        }
        let x = if is_f64(a) {
            unbox_f64(a)
        } else {
            unbox_int(a) as f64
        };
        let y = if is_f64(b) {
            unbox_f64(b)
        } else {
            unbox_int(b) as f64
        };
        return Some(match cmp {
            CMP_EQ => x == y,
            CMP_NE => x != y,
            CMP_LT => x < y,
            CMP_LE => x <= y,
            CMP_GT => x > y,
            CMP_GE => x >= y,
            _ => return None,
        });
    }
    if (cmp == CMP_IDENT || cmp == CMP_NOT_IDENT) && a < TAG_CONST && b < TAG_CONST {
        let eq = if is_f64(a) || is_f64(b) {
            is_f64(a) && is_f64(b) && unbox_f64(a) == unbox_f64(b)
        } else {
            a == b
        };
        return Some(if cmp == CMP_IDENT { eq } else { !eq });
    }
    None
}

/// Cold out-of-line release when overwriting a register that currently holds
/// `TAG_HEAP`. Refuses to overwrite a PHP reference in a named local.
#[cold]
#[inline(never)]
fn release_dst_slot(
    old: u64,
    dst: usize,
    local_count: u8,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
) -> bool {
    let idx = old as u32;
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

/// Write a scalar or `TAG_CONST` word into `r[dst]`, releasing any old heap
/// slot out-of-line (and bailing if `dst` holds a PHP reference).
#[inline(always)]
fn write_word(
    r: &mut [u64; 256],
    dst: usize,
    word: u64,
    local_count: u8,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
) -> bool {
    let old = r[dst];
    if old >= TAG_HEAP && !release_dst_slot(old, dst, local_count, heap, free_heap) {
        return false;
    }
    r[dst] = word;
    true
}

/// Write a heap `Value` into `r[dst]`, refusing to overwrite a PHP reference
/// in a named local.
#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn write_heap_value(
    r: &mut [u64; 256],
    dst: usize,
    value: Value,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
) -> bool {
    let old = r[dst];
    if old >= TAG_HEAP {
        let idx = old as u32 as usize;
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
    r[dst] = TAG_HEAP | u64::from(idx);
    *has_heap = true;
    true
}

/// Construct an owned `Value` from a NaN-boxed word without mutating `src`.
#[inline(always)]
fn read_owned_value(w: u64, heap: &[Option<Value>], consts: &[Value]) -> Option<Value> {
    if is_f64(w) {
        return Some(Value::Float(unbox_f64(w)));
    }
    match w & TAG_MASK {
        TAG_INT => Some(Value::Int(unbox_int(w))),
        TAG_BOOL => Some(Value::Bool((w & 1) != 0)),
        TAG_NULL => Some(Value::Null),
        TAG_CONST => consts.get(w as u32 as usize).cloned(),
        TAG_HEAP => {
            let val = heap.get(w as u32 as usize)?.as_ref()?;
            if val.is_reference() {
                None
            } else {
                Some(val.clone())
            }
        }
        _ => None,
    }
}

/// Out-of-line slow path for mixed int/float arithmetic, division, and overflow.
#[inline(never)]
fn num_binary_slow(kind: u8, a: u64, b: u64) -> Option<u64> {
    if is_int(a) && is_int(b) {
        let x = unbox_int(a);
        let y = unbox_int(b);
        return match kind {
            NUM_ADD => match x.checked_add(y) {
                Some(s) => box_int_checked(s),
                None => Some(box_f64((x as f64) + (y as f64))),
            },
            NUM_SUB => match x.checked_sub(y) {
                Some(s) => box_int_checked(s),
                None => Some(box_f64((x as f64) - (y as f64))),
            },
            NUM_MUL => match x.checked_mul(y) {
                Some(p) => box_int_checked(p),
                None => Some(box_f64((x as f64) * (y as f64))),
            },
            NUM_DIV => {
                if y == 0 {
                    return None;
                }
                if x % y == 0 {
                    box_int_checked(x / y)
                } else {
                    Some(box_f64((x as f64) / (y as f64)))
                }
            }
            _ => None,
        };
    }
    let a_num = is_f64(a) || is_int(a);
    let b_num = is_f64(b) || is_int(b);
    if !a_num || !b_num {
        return None;
    }
    let x = if is_f64(a) {
        unbox_f64(a)
    } else {
        unbox_int(a) as f64
    };
    let y = if is_f64(b) {
        unbox_f64(b)
    } else {
        unbox_int(b) as f64
    };
    let res = match kind {
        NUM_ADD => x + y,
        NUM_SUB => x - y,
        NUM_MUL => x * y,
        NUM_DIV => {
            if y == 0.0 {
                return None;
            }
            x / y
        }
        _ => return None,
    };
    Some(box_f64(res))
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn move_heap_slow(
    r: &mut [u64; 256],
    dst: usize,
    src_w: u64,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    let Some(val) = read_owned_value(src_w, heap, consts) else {
        return false;
    };
    write_heap_value(r, dst, val, local_count, has_heap, heap, free_heap)
}

#[inline(never)]
fn assign_take_heap_slow(
    r: &mut [u64; 256],
    dst: usize,
    src: usize,
    local_count: u8,
    has_heap: &mut bool,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
) -> bool {
    let src_w = r[src];
    if heap
        .get(src_w as u32 as usize)
        .and_then(Option::as_ref)
        .is_none_or(Value::is_reference)
    {
        return false;
    }
    let old_dst = r[dst];
    if old_dst >= TAG_HEAP
        && dst < local_count as usize
        && heap
            .get(old_dst as u32 as usize)
            .and_then(Option::as_ref)
            .is_some_and(Value::is_reference)
    {
        return false;
    }
    if dst != src {
        if old_dst >= TAG_HEAP
            && let Some(slot) = heap.get_mut(old_dst as u32 as usize)
        {
            *slot = None;
            free_heap.push(old_dst as u32);
        }
        r[dst] = src_w;
        r[src] = TAG_UNINIT;
        *has_heap = true;
    }
    true
}

#[inline(never)]
fn str_append_helper(
    r: &mut [u64; 256],
    loc: usize,
    src: usize,
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    let lw = r[loc];
    let rw = r[src];
    let mut stack_buf = [0u8; 32];
    let rhs_bytes: &[u8] = if (rw & TAG_MASK) == TAG_CONST {
        match consts.get(rw as u32 as usize) {
            Some(Value::InternedStr(s)) => s.as_bytes(),
            Some(Value::String(s)) => s.as_bytes(),
            _ => return false,
        }
    } else if rw >= TAG_HEAP {
        // Clone bytes only if `loc` and `src` could alias; `lower.rs` already
        // guards `loc != src`, and borrow checker wants split borrow on `heap`.
        let r_idx = rw as u32 as usize;
        if lw >= TAG_HEAP && (lw as u32 as usize) == r_idx {
            return false;
        }
        match heap.get(r_idx) {
            Some(Some(Value::InternedStr(s))) => s.as_bytes(),
            Some(Some(Value::String(s))) => {
                // Copy into a temporary Vec only if we're mutating `heap`.
                let bytes = s.as_bytes().to_vec();
                return str_append_owned_bytes(r, loc, &bytes, has_heap, heap, free_heap, consts);
            }
            _ => return false,
        }
    } else if is_int(rw) {
        let mut n = unbox_int(rw);
        let neg = n < 0;
        let mut v = n.unsigned_abs();
        let mut pos = stack_buf.len();
        loop {
            pos -= 1;
            stack_buf[pos] = b'0' + (v % 10) as u8;
            v /= 10;
            if v == 0 {
                break;
            }
        }
        if neg {
            pos -= 1;
            stack_buf[pos] = b'-';
        }
        let _ = &mut n;
        &stack_buf[pos..]
    } else {
        return false;
    };
    str_append_owned_bytes(r, loc, rhs_bytes, has_heap, heap, free_heap, consts)
}

fn str_append_owned_bytes(
    r: &mut [u64; 256],
    loc: usize,
    rhs_bytes: &[u8],
    has_heap: &mut bool,
    heap: &mut Vec<Option<Value>>,
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    let lw = r[loc];
    if lw >= TAG_HEAP {
        let idx = lw as u32 as usize;
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
    } else if (lw & TAG_MASK) == TAG_CONST {
        let lhs_bytes = match consts.get(lw as u32 as usize) {
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
        r[loc] = TAG_HEAP | u64::from(idx);
        *has_heap = true;
        true
    } else {
        false
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn str_len_helper(
    r: &mut [u64; 256],
    dst: usize,
    src: usize,
    local_count: u8,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
    consts: &[Value],
) -> bool {
    let sw = r[src];
    let len = if (sw & TAG_MASK) == TAG_CONST {
        match consts.get(sw as u32 as usize) {
            Some(Value::InternedStr(s)) => s.len(),
            Some(Value::String(s)) => s.len(),
            _ => return false,
        }
    } else if sw >= TAG_HEAP {
        match heap.get(sw as u32 as usize) {
            Some(Some(Value::String(s))) => s.len(),
            Some(Some(Value::InternedStr(s))) => s.len(),
            _ => return false,
        }
    } else {
        return false;
    };
    let Some(w) = box_int_checked(len as i64) else {
        return false;
    };
    write_word(r, dst, w, local_count, heap, free_heap)
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn new_array_helper(
    r: &mut [u64; 256],
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
    write_heap_value(
        r,
        dst,
        Value::Array(Rc::new(array)),
        local_count,
        has_heap,
        heap,
        free_heap,
    )
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_get_int_helper(
    r: &mut [u64; 256],
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
    let idx_w = r[idx_r];
    if !is_int(idx_w) {
        return false;
    }
    let key = unbox_int(idx_w);
    let base_w = r[arr_r];

    enum Found {
        Word(u64),
        Heap(Value),
        Bail,
    }

    let resolve_val = |v: &Value| -> Found {
        match v {
            Value::Int(i) => match box_int_checked(*i) {
                Some(w) => Found::Word(w),
                None => Found::Heap(Value::Int(*i)),
            },
            Value::Float(f) => Found::Word(box_f64(*f)),
            Value::Bool(b) => Found::Word(box_bool(*b)),
            Value::Null => Found::Word(TAG_NULL),
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

    let found = if base_w >= TAG_HEAP {
        match heap.get(base_w as u32 as usize) {
            Some(Some(Value::Array(rc))) => match rc.get_int(key) {
                Some(v) => resolve_val(v),
                None if quiet => Found::Word(TAG_NULL),
                None => Found::Bail,
            },
            Some(Some(Value::InternedStr(s))) => resolve_bytes(s.as_bytes()),
            Some(Some(Value::String(s))) => resolve_bytes(s.as_bytes()),
            Some(Some(Value::Reference(cell))) => {
                if let Ok(guard) = cell.try_borrow() {
                    match &*guard {
                        Value::Array(rc) => match rc.get_int(key) {
                            Some(v) => resolve_val(v),
                            None if quiet => Found::Word(TAG_NULL),
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
    } else if (base_w & TAG_MASK) == TAG_CONST {
        match consts.get(base_w as u32 as usize) {
            Some(Value::InternedStr(s)) => resolve_bytes(s.as_bytes()),
            Some(Value::String(s)) => resolve_bytes(s.as_bytes()),
            Some(Value::Array(rc)) => match rc.get_int(key) {
                Some(v) => resolve_val(v),
                None if quiet => Found::Word(TAG_NULL),
                None => Found::Bail,
            },
            _ => Found::Bail,
        }
    } else {
        Found::Bail
    };

    match found {
        Found::Word(w) => write_word(r, dst, w, local_count, heap, free_heap),
        Found::Heap(val) => write_heap_value(r, dst, val, local_count, has_heap, heap, free_heap),
        Found::Bail => false,
    }
}

#[inline(never)]
#[allow(clippy::too_many_arguments)]
fn arr_2d_get_int_helper(
    r: &mut [u64; 256],
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
    let base_w = r[arr_r];
    let i1_w = r[idx1_r];
    let i2_w = r[idx2_r];
    if base_w < TAG_HEAP || !is_int(i1_w) || !is_int(i2_w) {
        return false;
    }
    let k1 = unbox_int(i1_w);
    let k2 = unbox_int(i2_w);

    enum Found2 {
        Word(u64),
        Heap(Value),
        Bail,
    }

    let found = match heap.get(base_w as u32 as usize) {
        Some(Some(Value::Array(rc1))) => match rc1.get_int(k1) {
            Some(Value::Array(rc2)) => match rc2.get_int(k2) {
                Some(Value::Int(i)) => match box_int_checked(*i) {
                    Some(w) => Found2::Word(w),
                    None => Found2::Heap(Value::Int(*i)),
                },
                Some(Value::Float(f)) => Found2::Word(box_f64(*f)),
                Some(Value::Bool(b)) => Found2::Word(box_bool(*b)),
                Some(Value::Null) => Found2::Word(TAG_NULL),
                Some(other) if !other.is_reference() => Found2::Heap(other.clone()),
                None if quiet => Found2::Word(TAG_NULL),
                _ => Found2::Bail,
            },
            None if quiet => Found2::Word(TAG_NULL),
            _ => Found2::Bail,
        },
        _ => Found2::Bail,
    };

    match found {
        Found2::Word(w) => write_word(r, dst, w, local_count, heap, free_heap),
        Found2::Heap(val) => write_heap_value(r, dst, val, local_count, has_heap, heap, free_heap),
        Found2::Bail => false,
    }
}

#[inline(never)]
fn arr_set_int_helper(
    r: &[u64; 256],
    arr_r: usize,
    idx_r: usize,
    src_r: usize,
    heap: &mut [Option<Value>],
    consts: &[Value],
) -> bool {
    let base_w = r[arr_r];
    let idx_w = r[idx_r];
    if base_w < TAG_HEAP || !is_int(idx_w) {
        return false;
    }
    let key = unbox_int(idx_w);
    let Some(val) = read_owned_value(r[src_r], heap, consts) else {
        return false;
    };
    match heap.get_mut(base_w as u32 as usize) {
        Some(Some(Value::Array(rc))) => Rc::make_mut(rc).insert_int_if_not_ref(key, val).is_ok(),
        Some(Some(Value::Reference(cell))) => {
            if let Ok(mut guard) = cell.try_borrow_mut() {
                match &mut *guard {
                    Value::Array(rc) => Rc::make_mut(rc).insert_int_if_not_ref(key, val).is_ok(),
                    _ => false,
                }
            } else {
                false
            }
        }
        _ => false,
    }
}

#[inline(never)]
fn arr_add_assign_int_helper(
    r: &[u64; 256],
    arr_r: usize,
    idx_r: usize,
    src_r: usize,
    heap: &mut [Option<Value>],
) -> bool {
    let base_w = r[arr_r];
    let idx_w = r[idx_r];
    let rhs_w = r[src_r];
    if base_w < TAG_HEAP || !is_int(idx_w) {
        return false;
    }
    let rhs_is_int = is_int(rhs_w);
    let rhs_is_f64 = is_f64(rhs_w);
    if !rhs_is_int && !rhs_is_f64 {
        return false;
    }
    let key = unbox_int(idx_w);
    let Some(Some(Value::Array(rc))) = heap.get_mut(base_w as u32 as usize) else {
        return false;
    };
    let target = Rc::make_mut(rc);
    let Some(slot) = target.get_mut_int(key) else {
        return false;
    };
    match (slot, rhs_is_int) {
        (slot @ Value::Int(_), true) => {
            let Value::Int(lhs) = *slot else {
                unreachable!()
            };
            let rhs = unbox_int(rhs_w);
            match lhs.checked_add(rhs) {
                Some(sum) => *slot = Value::Int(sum),
                None => *slot = Value::Float((lhs as f64) + (rhs as f64)),
            }
            true
        }
        (slot @ Value::Float(_), false) => {
            let Value::Float(lhs) = *slot else {
                unreachable!()
            };
            *slot = Value::Float(lhs + unbox_f64(rhs_w));
            true
        }
        (slot @ Value::Float(_), true) => {
            let Value::Float(lhs) = *slot else {
                unreachable!()
            };
            *slot = Value::Float(lhs + (unbox_int(rhs_w) as f64));
            true
        }
        (slot @ Value::Int(_), false) => {
            let Value::Int(lhs) = *slot else {
                unreachable!()
            };
            *slot = Value::Float((lhs as f64) + unbox_f64(rhs_w));
            true
        }
        _ => false,
    }
}

/// Fused `$dst_arr[$idx] += $src_arr[$idx]` (single op for `ary3`!).
#[inline(never)]
fn arr_add_from_arr_helper(
    r: &[u64; 256],
    dst_arr_r: usize,
    src_arr_r: usize,
    idx_r: usize,
    heap: &mut [Option<Value>],
) -> bool {
    let dst_w = r[dst_arr_r];
    let src_w = r[src_arr_r];
    let idx_w = r[idx_r];
    if dst_w < TAG_HEAP || src_w < TAG_HEAP || !is_int(idx_w) {
        return false;
    }
    let key = unbox_int(idx_w);
    enum RhsNum {
        Int(i64),
        Float(f64),
    }
    let rhs = match heap.get(src_w as u32 as usize) {
        Some(Some(Value::Array(rc))) => match rc.get_int(key) {
            Some(Value::Int(i)) => RhsNum::Int(*i),
            Some(Value::Float(f)) => RhsNum::Float(*f),
            _ => return false,
        },
        _ => return false,
    };
    let Some(Some(Value::Array(rc))) = heap.get_mut(dst_w as u32 as usize) else {
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

#[inline(never)]
fn arr_push_helper(
    r: &[u64; 256],
    arr_r: usize,
    src_r: usize,
    heap: &mut [Option<Value>],
    consts: &[Value],
) -> bool {
    let base_w = r[arr_r];
    if base_w < TAG_HEAP {
        return false;
    }
    let Some(val) = read_owned_value(r[src_r], heap, consts) else {
        return false;
    };
    match heap.get_mut(base_w as u32 as usize) {
        Some(Some(Value::Array(rc))) => Rc::make_mut(rc).push(val).is_ok(),
        _ => false,
    }
}

#[inline(never)]
fn arr_unset_int_helper(
    r: &[u64; 256],
    arr_r: usize,
    idx_r: usize,
    heap: &mut [Option<Value>],
) -> bool {
    let base_w = r[arr_r];
    let idx_w = r[idx_r];
    if base_w < TAG_HEAP || !is_int(idx_w) {
        return false;
    }
    let key = unbox_int(idx_w);
    match heap.get_mut(base_w as u32 as usize) {
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

/// Out-of-line general `CallFast` setup helper (keeps the hot dispatch loop
/// leaf-clean!).
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
        let aw = fast_stack.regs[base + args_base + i];
        if aw == TAG_UNINIT {
            return CallSetup::Bail;
        }
        if aw >= TAG_HEAP {
            let is_ref = fast_stack
                .heap
                .get(aw as u32 as usize)
                .and_then(Option::as_ref)
                .is_none_or(Value::is_reference);
            if is_ref {
                return CallSetup::Bail;
            }
            callee_has_heap = true;
        }
        if (typed & 1) != 0 {
            let mask = callee.param_masks[i];
            if !word_type_mask(aw, &fast_stack.heap, consts).is_some_and(|own| mask.intersects(own))
            {
                return CallSetup::Bail;
            }
        }
        typed >>= 1;
    }

    // Empty-body `[ReturnNull]` shortcut inside Tier 2!
    if callee.is_empty_return_null
        && callee
            .return_mask
            .is_none_or(|m| return_mask_fits(m, TAG_NULL, &fast_stack.heap, consts))
    {
        let old_dst = fast_stack.regs[base + dst as usize];
        if old_dst >= TAG_HEAP
            && !release_dst_slot(
                old_dst,
                dst as usize,
                cur_fn.local_count,
                &mut fast_stack.heap,
                &mut fast_stack.free_heap,
            )
        {
            return CallSetup::Bail;
        }
        for i in 0..count {
            let aw = fast_stack.regs[base + args_base + i];
            if aw >= TAG_HEAP {
                fast_stack.free_heap_slot(aw as u32);
            }
            fast_stack.regs[base + args_base + i] = TAG_UNINIT;
        }
        fast_stack.regs[base + dst as usize] = TAG_NULL;
        return CallSetup::EmptyDone;
    }

    let new_base = base + cur_fn.reg_count as usize;
    fast_stack.ensure_window(new_base);

    let mut args_buf = [TAG_UNINIT; 16];
    for (i, slot) in args_buf.iter_mut().enumerate().take(count) {
        *slot = fast_stack.regs[base + args_base + i];
        fast_stack.regs[base + args_base + i] = TAG_UNINIT;
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

    fast_stack.regs[new_base..new_base + count].copy_from_slice(&args_buf[..count]);
    let callee_regs = callee.reg_count as usize;
    if callee_regs > count {
        fast_stack.regs[new_base + count..new_base + callee_regs].fill(TAG_UNINIT);
    }

    CallSetup::Enter {
        callee_idx,
        callee,
        new_base,
        callee_has_heap,
    }
}

/// Out-of-line slow path for `Op::CallNoop` when an argument is `TAG_HEAP`
/// or `count > 1` or `dst` holds `TAG_HEAP`.
#[inline(never)]
fn call_noop_slow(
    r: &mut [u64; 256],
    dst: usize,
    args_base: usize,
    count: usize,
    local_count: u8,
    heap: &mut [Option<Value>],
    free_heap: &mut Vec<u32>,
) -> bool {
    for i in 0..count {
        let aw = r[args_base + i];
        if aw == TAG_UNINIT {
            return false;
        }
        if aw >= TAG_HEAP
            && heap
                .get(aw as u32 as usize)
                .and_then(Option::as_ref)
                .is_none_or(Value::is_reference)
        {
            return false;
        }
    }
    let old_dst = r[dst];
    if old_dst >= TAG_HEAP && !release_dst_slot(old_dst, dst, local_count, heap, free_heap) {
        return false;
    }
    for i in 0..count {
        let aw = r[args_base + i];
        if aw >= TAG_HEAP
            && let Some(slot) = heap.get_mut(aw as u32 as usize)
        {
            *slot = None;
            free_heap.push(aw as u32);
        }
        r[args_base + i] = TAG_UNINIT;
    }
    r[dst] = TAG_NULL;
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

    // All `CallDirect` callees belong to the same `CompiledUnit` and share the
    // exact same immutable constant table.
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

    'frame_loop: loop {
        fast_stack.ensure_window(base);
        let r: &mut [u64; 256] = (&mut fast_stack.regs[base..base + 256])
            .try_into()
            .expect("256-reg window");
        let heap = &mut fast_stack.heap;
        let free_heap = &mut fast_stack.free_heap;

        loop {
            let Some(&op) = cur_ops.get(ip) else {
                break 'frame_loop;
            };

            match op.op {
                // =============================================================
                // FUSED ROTATED-LOOP LATCHES (1 op per loop round!)
                // =============================================================
                Op::IncLoopLtImm => {
                    let idx = op.a as usize;
                    let w = r[idx];
                    if is_int(w) {
                        let next = unbox_int(w) + 1;
                        if let Some(nw) = box_int_checked(next) {
                            r[idx] = nw;
                            if next < i64::from(op.imm) {
                                ip = (op.b as usize) | ((op.c as usize) << 8);
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
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
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        YIELD_AFTER_STEPS as u64,
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
                    let idx = op.a as usize;
                    let w = r[idx];
                    if is_int(w) {
                        let next = unbox_int(w) + 1;
                        if let Some(nw) = box_int_checked(next) {
                            r[idx] = nw;
                            if next <= i64::from(op.imm) {
                                ip = (op.b as usize) | ((op.c as usize) << 8);
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
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
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        YIELD_AFTER_STEPS as u64,
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
                    let idx = op.a as usize;
                    let w = r[idx];
                    if is_int(w) {
                        let next = unbox_int(w) - 1;
                        if let Some(nw) = box_int_checked(next) {
                            r[idx] = nw;
                            if next >= i64::from(op.imm) {
                                ip = (op.b as usize) | ((op.c as usize) << 8);
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
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
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        YIELD_AFTER_STEPS as u64,
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
                    let idx = op.a as usize;
                    let w = r[idx];
                    if is_int(w) {
                        let next = unbox_int(w) - 1;
                        if let Some(nw) = box_int_checked(next) {
                            r[idx] = nw;
                            if next > i64::from(op.imm) {
                                ip = (op.b as usize) | ((op.c as usize) << 8);
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
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
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        YIELD_AFTER_STEPS as u64,
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
                    let idx = op.a as usize;
                    let w = r[idx];
                    let lim = r[op.b as usize];
                    if is_int(w) && is_int(lim) {
                        let next = unbox_int(w) + 1;
                        if let Some(nw) = box_int_checked(next) {
                            r[idx] = nw;
                            if next < unbox_int(lim) {
                                ip = op.imm as usize;
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
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
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        YIELD_AFTER_STEPS as u64,
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
                    let idx = op.a as usize;
                    let w = r[idx];
                    let lim = r[op.b as usize];
                    if is_int(w) && is_int(lim) {
                        let next = unbox_int(w) + 1;
                        if let Some(nw) = box_int_checked(next) {
                            r[idx] = nw;
                            if next <= unbox_int(lim) {
                                ip = op.imm as usize;
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
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
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        YIELD_AFTER_STEPS as u64,
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
                    let idx = op.a as usize;
                    let w = r[idx];
                    let lim = r[op.b as usize];
                    if is_int(w) && is_int(lim) {
                        let next = unbox_int(w) - 1;
                        if let Some(nw) = box_int_checked(next) {
                            r[idx] = nw;
                            if next >= unbox_int(lim) {
                                ip = op.imm as usize;
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
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
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        YIELD_AFTER_STEPS as u64,
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
                    let idx = op.a as usize;
                    let w = r[idx];
                    let lim = r[op.b as usize];
                    if is_int(w) && is_int(lim) {
                        let next = unbox_int(w) - 1;
                        if let Some(nw) = box_int_checked(next) {
                            r[idx] = nw;
                            if next > unbox_int(lim) {
                                ip = op.imm as usize;
                                work += 1;
                                if work >= YIELD_AFTER_STEPS {
                                    work = 0;
                                    return materialize_deopt(
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
                                        true,
                                        work_io,
                                        executed_io,
                                        work,
                                        YIELD_AFTER_STEPS as u64,
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
                // FLOAT-FIRST ARITHMETIC (`mandel`, `mandel2`, `heapsort`)
                // =============================================================
                Op::AddF64 => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    let res = if is_f64(a) && is_f64(b) {
                        box_f64(unbox_f64(a) + unbox_f64(b))
                    } else if let Some(w) = num_binary_slow(NUM_ADD, a, b) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubF64 => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    let res = if is_f64(a) && is_f64(b) {
                        box_f64(unbox_f64(a) - unbox_f64(b))
                    } else if let Some(w) = num_binary_slow(NUM_SUB, a, b) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulF64 => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    let res = if is_f64(a) && is_f64(b) {
                        box_f64(unbox_f64(a) * unbox_f64(b))
                    } else if let Some(w) = num_binary_slow(NUM_MUL, a, b) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::DivF64 => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    let res = if is_f64(a) && is_f64(b) {
                        let y = unbox_f64(b);
                        if y == 0.0 {
                            break 'frame_loop;
                        }
                        box_f64(unbox_f64(a) / y)
                    } else if let Some(w) = num_binary_slow(NUM_DIV, a, b) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulF64Imm => {
                    let a = r[op.b as usize];
                    let res = if is_f64(a) {
                        box_f64(unbox_f64(a) * (op.imm as f64))
                    } else if let Some(w) = num_binary_slow(NUM_MUL, a, box_i32(op.imm)) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::AddF64Imm => {
                    let a = r[op.b as usize];
                    let res = if is_f64(a) {
                        box_f64(unbox_f64(a) + (op.imm as f64))
                    } else if let Some(w) = num_binary_slow(NUM_ADD, a, box_i32(op.imm)) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubF64Imm => {
                    let a = r[op.b as usize];
                    let res = if is_f64(a) {
                        box_f64(unbox_f64(a) - (op.imm as f64))
                    } else if let Some(w) = num_binary_slow(NUM_SUB, a, box_i32(op.imm)) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // INT-FIRST ARITHMETIC (out-of-line float/overflow fallback)
                // =============================================================
                Op::AddNum => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    let res = if is_int(a) && is_int(b) {
                        match box_int_checked(unbox_int(a) + unbox_int(b)) {
                            Some(w) => w,
                            None => break 'frame_loop,
                        }
                    } else if let Some(w) = num_binary_slow(NUM_ADD, a, b) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubNum => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    let res = if is_int(a) && is_int(b) {
                        match box_int_checked(unbox_int(a) - unbox_int(b)) {
                            Some(w) => w,
                            None => break 'frame_loop,
                        }
                    } else if let Some(w) = num_binary_slow(NUM_SUB, a, b) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulNum => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    let res = if is_int(a) && is_int(b) {
                        match unbox_int(a)
                            .checked_mul(unbox_int(b))
                            .and_then(box_int_checked)
                        {
                            Some(w) => w,
                            None => match num_binary_slow(NUM_MUL, a, b) {
                                Some(w) => w,
                                None => break 'frame_loop,
                            },
                        }
                    } else if let Some(w) = num_binary_slow(NUM_MUL, a, b) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::DivNum => {
                    let Some(res) = num_binary_slow(NUM_DIV, r[op.b as usize], r[op.c as usize])
                    else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ModInt => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    if !is_int(a) || !is_int(b) {
                        break 'frame_loop;
                    }
                    let y = unbox_int(b);
                    if y == 0 {
                        break 'frame_loop;
                    }
                    let rem = unbox_int(a) % y;
                    let res = TAG_INT | ((rem as u64) & PAYLOAD_MASK);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitAnd => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    if !is_int(a) || !is_int(b) {
                        break 'frame_loop;
                    }
                    if !write_word(r, op.a as usize, a & b, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitOr => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    if !is_int(a) || !is_int(b) {
                        break 'frame_loop;
                    }
                    if !write_word(r, op.a as usize, a | b, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitXor => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    if !is_int(a) || !is_int(b) {
                        break 'frame_loop;
                    }
                    let res = TAG_INT | ((a ^ b) & PAYLOAD_MASK);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Shl => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    if !is_int(a) || !is_int(b) {
                        break 'frame_loop;
                    }
                    let s = unbox_int(b);
                    if s < 0 {
                        break 'frame_loop;
                    }
                    let shifted = if s >= 64 {
                        0
                    } else {
                        unbox_int(a).wrapping_shl(s as u32)
                    };
                    let Some(res) = box_int_checked(shifted) else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Shr => {
                    let a = r[op.b as usize];
                    let b = r[op.c as usize];
                    if !is_int(a) || !is_int(b) {
                        break 'frame_loop;
                    }
                    let s = unbox_int(b);
                    if s < 0 {
                        break 'frame_loop;
                    }
                    let x = unbox_int(a);
                    let shifted = if s >= 64 { x >> 63 } else { x >> (s as u32) };
                    let res = TAG_INT | ((shifted as u64) & PAYLOAD_MASK);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // IMMEDIATE ARITHMETIC
                // =============================================================
                Op::AddImm => {
                    let a = r[op.b as usize];
                    let res = if is_int(a) {
                        match box_int_checked(unbox_int(a) + i64::from(op.imm)) {
                            Some(w) => w,
                            None => break 'frame_loop,
                        }
                    } else if let Some(w) = num_binary_slow(NUM_ADD, a, box_i32(op.imm)) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubImm => {
                    let a = r[op.b as usize];
                    let res = if is_int(a) {
                        match box_int_checked(unbox_int(a) - i64::from(op.imm)) {
                            Some(w) => w,
                            None => break 'frame_loop,
                        }
                    } else if let Some(w) = num_binary_slow(NUM_SUB, a, box_i32(op.imm)) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulImm => {
                    let a = r[op.b as usize];
                    let res = if is_int(a) {
                        match unbox_int(a)
                            .checked_mul(i64::from(op.imm))
                            .and_then(box_int_checked)
                        {
                            Some(w) => w,
                            None => match num_binary_slow(NUM_MUL, a, box_i32(op.imm)) {
                                Some(w) => w,
                                None => break 'frame_loop,
                            },
                        }
                    } else if let Some(w) = num_binary_slow(NUM_MUL, a, box_i32(op.imm)) {
                        w
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ModImm => {
                    let a = r[op.b as usize];
                    if !is_int(a) || op.imm == 0 {
                        break 'frame_loop;
                    }
                    let rem = unbox_int(a) % i64::from(op.imm);
                    let res = TAG_INT | ((rem as u64) & PAYLOAD_MASK);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitAndImm => {
                    let a = r[op.b as usize];
                    if !is_int(a) {
                        break 'frame_loop;
                    }
                    let res = a & box_i32(op.imm);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitOrImm => {
                    let a = r[op.b as usize];
                    if !is_int(a) {
                        break 'frame_loop;
                    }
                    let res = a | box_i32(op.imm);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitXorImm => {
                    let a = r[op.b as usize];
                    if !is_int(a) {
                        break 'frame_loop;
                    }
                    let res = TAG_INT | ((a ^ box_i32(op.imm)) & PAYLOAD_MASK);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ShlImm => {
                    let a = r[op.b as usize];
                    if !is_int(a) || op.imm < 0 {
                        break 'frame_loop;
                    }
                    let shifted = if op.imm >= 64 {
                        0
                    } else {
                        unbox_int(a).wrapping_shl(op.imm as u32)
                    };
                    let Some(res) = box_int_checked(shifted) else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ShrImm => {
                    let a = r[op.b as usize];
                    if !is_int(a) || op.imm < 0 {
                        break 'frame_loop;
                    }
                    let x = unbox_int(a);
                    let shifted = if op.imm >= 64 {
                        x >> 63
                    } else {
                        x >> (op.imm as u32)
                    };
                    let res = TAG_INT | ((shifted as u64) & PAYLOAD_MASK);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // COMPOUND LOCAL UPDATES
                // =============================================================
                Op::AddAssign => {
                    let loc = op.a as usize;
                    let a = r[loc];
                    let b = r[op.b as usize];
                    if is_int(a) && is_int(b) {
                        let Some(res) = box_int_checked(unbox_int(a) + unbox_int(b)) else {
                            break 'frame_loop;
                        };
                        r[loc] = res;
                    } else if is_f64(a) && is_f64(b) {
                        r[loc] = box_f64(unbox_f64(a) + unbox_f64(b));
                    } else if let Some(res) = num_binary_slow(NUM_ADD, a, b) {
                        r[loc] = res;
                    } else {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::SubAssign => {
                    let loc = op.a as usize;
                    let a = r[loc];
                    let b = r[op.b as usize];
                    if is_int(a) && is_int(b) {
                        let Some(res) = box_int_checked(unbox_int(a) - unbox_int(b)) else {
                            break 'frame_loop;
                        };
                        r[loc] = res;
                    } else if is_f64(a) && is_f64(b) {
                        r[loc] = box_f64(unbox_f64(a) - unbox_f64(b));
                    } else if let Some(res) = num_binary_slow(NUM_SUB, a, b) {
                        r[loc] = res;
                    } else {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::MulAssign => {
                    let loc = op.a as usize;
                    let a = r[loc];
                    let b = r[op.b as usize];
                    if is_int(a) && is_int(b) {
                        let res = match unbox_int(a)
                            .checked_mul(unbox_int(b))
                            .and_then(box_int_checked)
                        {
                            Some(w) => w,
                            None => match num_binary_slow(NUM_MUL, a, b) {
                                Some(w) => w,
                                None => break 'frame_loop,
                            },
                        };
                        r[loc] = res;
                    } else if is_f64(a) && is_f64(b) {
                        r[loc] = box_f64(unbox_f64(a) * unbox_f64(b));
                    } else if let Some(res) = num_binary_slow(NUM_MUL, a, b) {
                        r[loc] = res;
                    } else {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::StrAppend => {
                    if !str_append_helper(
                        r,
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
                    if !write_word(
                        r,
                        op.a as usize,
                        box_i32(op.imm),
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadF64 => {
                    let bits = cur_f64[op.imm as usize];
                    if !write_word(r, op.a as usize, bits, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadBool => {
                    let w = TAG_BOOL | u64::from(op.b);
                    if !write_word(r, op.a as usize, w, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadNull => {
                    if !write_word(r, op.a as usize, TAG_NULL, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::LoadConstRef => {
                    let w = TAG_CONST | (op.imm as u32 as u64);
                    if !write_word(r, op.a as usize, w, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::NewArray => {
                    if !new_array_helper(
                        r,
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
                    let sw = r[op.b as usize];
                    if sw < TAG_UNINIT {
                        if !write_word(r, dst, sw, cur_local_count, heap, free_heap) {
                            break 'frame_loop;
                        }
                    } else if sw >= TAG_HEAP {
                        if !move_heap_slow(
                            r,
                            dst,
                            sw,
                            cur_local_count,
                            &mut cur_has_heap,
                            heap,
                            free_heap,
                            unit_consts,
                        ) {
                            break 'frame_loop;
                        }
                    } else {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::AssignTake => {
                    let dst = op.a as usize;
                    let src = op.b as usize;
                    let sw = r[src];
                    if sw < TAG_UNINIT {
                        if !write_word(r, dst, sw, cur_local_count, heap, free_heap) {
                            break 'frame_loop;
                        }
                        if dst != src {
                            r[src] = TAG_UNINIT;
                        }
                    } else if sw >= TAG_HEAP {
                        if !assign_take_heap_slow(
                            r,
                            dst,
                            src,
                            cur_local_count,
                            &mut cur_has_heap,
                            heap,
                            free_heap,
                        ) {
                            break 'frame_loop;
                        }
                    } else {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::Clear => {
                    let idx = op.a as usize;
                    let old = r[idx];
                    if old >= TAG_HEAP
                        && let Some(slot) = heap.get_mut(old as u32 as usize)
                    {
                        *slot = None;
                        free_heap.push(old as u32);
                    }
                    r[idx] = TAG_UNINIT;
                    ip += 1;
                }

                // =============================================================
                // UNARY, CASTS, BUILTINS, INC/DEC
                // =============================================================
                Op::NegNum => {
                    let a = r[op.b as usize];
                    let res = if is_f64(a) {
                        box_f64(-unbox_f64(a))
                    } else if is_int(a) {
                        match box_int_checked(-unbox_int(a)) {
                            Some(w) => w,
                            None => break 'frame_loop,
                        }
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::NotBool => {
                    let Some(truthy) = scalar_truthy(r[op.b as usize]) else {
                        break 'frame_loop;
                    };
                    if !write_word(
                        r,
                        op.a as usize,
                        box_bool(!truthy),
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::BitNotInt => {
                    let a = r[op.b as usize];
                    if !is_int(a) {
                        break 'frame_loop;
                    }
                    let res = TAG_INT | ((!a) & PAYLOAD_MASK);
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::CastInt => {
                    let a = r[op.b as usize];
                    let res = if is_int(a) {
                        a
                    } else if is_f64(a) {
                        let f = unbox_f64(a);
                        if f.is_finite()
                            && (-140_737_488_355_328.0..140_737_488_355_328.0).contains(&f)
                        {
                            match box_int_checked(f as i64) {
                                Some(w) => w,
                                None => break 'frame_loop,
                            }
                        } else {
                            break 'frame_loop;
                        }
                    } else if a == TAG_NULL {
                        box_i32(0)
                    } else if (a & TAG_MASK) == TAG_BOOL {
                        box_i32((a & 1) as i32)
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::CastFloat => {
                    let a = r[op.b as usize];
                    let res = if is_f64(a) {
                        a
                    } else if is_int(a) {
                        box_f64(unbox_int(a) as f64)
                    } else if a == TAG_NULL {
                        box_f64(0.0)
                    } else if (a & TAG_MASK) == TAG_BOOL {
                        box_f64(if (a & 1) != 0 { 1.0 } else { 0.0 })
                    } else {
                        break 'frame_loop;
                    };
                    if !write_word(r, op.a as usize, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::CastBool => {
                    let Some(truthy) = scalar_truthy(r[op.b as usize]) else {
                        break 'frame_loop;
                    };
                    if !write_word(
                        r,
                        op.a as usize,
                        box_bool(truthy),
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::StrLen => {
                    if !str_len_helper(
                        r,
                        op.a as usize,
                        op.b as usize,
                        cur_local_count,
                        heap,
                        free_heap,
                        unit_consts,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::IncLocalFast => {
                    let loc = op.a as usize;
                    let w = r[loc];
                    if is_int(w) {
                        let Some(nw) = box_int_checked(unbox_int(w) + 1) else {
                            break 'frame_loop;
                        };
                        r[loc] = nw;
                        ip += 1;
                    } else if is_f64(w) {
                        r[loc] = box_f64(unbox_f64(w) + 1.0);
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::DecLocalFast => {
                    let loc = op.a as usize;
                    let w = r[loc];
                    if is_int(w) {
                        let Some(nw) = box_int_checked(unbox_int(w) - 1) else {
                            break 'frame_loop;
                        };
                        r[loc] = nw;
                        ip += 1;
                    } else if is_f64(w) {
                        r[loc] = box_f64(unbox_f64(w) - 1.0);
                        ip += 1;
                    } else {
                        break 'frame_loop;
                    }
                }
                Op::IncDecLocal => {
                    let dst = op.a as usize;
                    let loc = op.b as usize;
                    let mode = op.c; // 0=PreInc, 1=PostInc, 2=PreDec, 3=PostDec
                    let old = r[loc];
                    let next = if is_int(old) {
                        let delta = if mode < 2 { 1 } else { -1 };
                        let Some(nw) = box_int_checked(unbox_int(old) + delta) else {
                            break 'frame_loop;
                        };
                        nw
                    } else if is_f64(old) {
                        let delta = if mode < 2 { 1.0 } else { -1.0 };
                        box_f64(unbox_f64(old) + delta)
                    } else {
                        break 'frame_loop;
                    };
                    let res = if (mode & 1) == 0 { next } else { old };
                    // Validate/release `dst` BEFORE mutating `loc`!
                    if !write_word(r, dst, res, cur_local_count, heap, free_heap) {
                        break 'frame_loop;
                    }
                    r[loc] = next;
                    ip += 1;
                }

                // =============================================================
                // COMPARISONS & BRANCHES
                // =============================================================
                Op::CmpNum => {
                    let Some(res) = compare_words(op.imm as u8, r[op.b as usize], r[op.c as usize])
                    else {
                        break 'frame_loop;
                    };
                    if !write_word(
                        r,
                        op.a as usize,
                        box_bool(res),
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
                            return materialize_deopt(
                                code,
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
                                YIELD_AFTER_STEPS as u64,
                            );
                        }
                    }
                    ip = target;
                }
                Op::JumpIf | Op::JumpIfNot => {
                    let Some(truthy) = scalar_truthy(r[op.a as usize]) else {
                        break 'frame_loop;
                    };
                    let take = if op.op == Op::JumpIf { truthy } else { !truthy };
                    if take {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    code,
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
                                    YIELD_AFTER_STEPS as u64,
                                );
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }
                Op::JumpIfNull | Op::JumpIfNotNull => {
                    let w = r[op.a as usize];
                    if w == TAG_UNINIT || w >= TAG_HEAP {
                        break 'frame_loop;
                    }
                    let is_null = w == TAG_NULL;
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
                                return materialize_deopt(
                                    code,
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
                                    YIELD_AFTER_STEPS as u64,
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
                    let Some(res) = compare_words(cmp, r[op.b as usize], r[op.c as usize]) else {
                        break 'frame_loop;
                    };
                    if res == when {
                        let target = op.imm as usize;
                        if target <= ip {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    code,
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
                                    YIELD_AFTER_STEPS as u64,
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
                    let lw = r[op.a as usize];
                    let imm = i64::from(op.imm);
                    let res = if is_int(lw) {
                        let a = unbox_int(lw);
                        match op.op {
                            Op::BranchLtImm => a < imm,
                            Op::BranchLeImm => a <= imm,
                            Op::BranchGtImm => a > imm,
                            Op::BranchGeImm => a >= imm,
                            Op::BranchEqImm | Op::BranchIdentImm => a == imm,
                            Op::BranchNeImm | Op::BranchNotIdentImm => a != imm,
                            _ => unreachable!(),
                        }
                    } else if is_f64(lw) {
                        let a = unbox_f64(lw);
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
                        && lw < TAG_UNINIT
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
                                    code,
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
                                    YIELD_AFTER_STEPS as u64,
                                );
                            }
                        }
                        ip = target;
                    } else {
                        ip += 1;
                    }
                }

                // =============================================================
                // IN-LOOP ARRAY OPS (ALL OUT-OF-LINE HELPERS!)
                // =============================================================
                Op::ArrGetInt => {
                    if !arr_get_int_helper(
                        r,
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
                        r,
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
                        r,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        heap,
                        unit_consts,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrAddAssignInt => {
                    if !arr_add_assign_int_helper(
                        r,
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
                        r,
                        op.a as usize,
                        op.b as usize,
                        op.c as usize,
                        heap,
                    ) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrPush => {
                    if !arr_push_helper(r, op.a as usize, op.b as usize, heap, unit_consts) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }
                Op::ArrUnsetInt => {
                    if !arr_unset_int_helper(r, op.a as usize, op.b as usize, heap) {
                        break 'frame_loop;
                    }
                    ip += 1;
                }

                // =============================================================
                // SPECIALIZED CALLS & RETURNS (ZERO-OVERHEAD RECURSION & NOOPS!)
                // =============================================================
                Op::CallNoop => {
                    let dst = op.a as usize;
                    let args_base = op.b as usize;
                    let count = op.c as usize;
                    if count == 1 && r[args_base] < TAG_UNINIT && r[dst] < TAG_HEAP {
                        r[args_base] = TAG_UNINIT;
                        r[dst] = TAG_NULL;
                    } else if count == 0 && r[dst] < TAG_HEAP {
                        r[dst] = TAG_NULL;
                    } else if !call_noop_slow(
                        r,
                        dst,
                        args_base,
                        count,
                        cur_local_count,
                        heap,
                        free_heap,
                    ) {
                        break 'frame_loop;
                    }
                    work += 1;
                    if work >= YIELD_AFTER_STEPS {
                        work = 0;
                        return materialize_deopt(
                            code,
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
                            YIELD_AFTER_STEPS as u64,
                        );
                    }
                    ip += 1;
                }
                Op::CallSelf1 => {
                    let arg0 = r[op.b as usize];
                    if arg0 < TAG_UNINIT && fast_stack.frames.len() < max_fast_frames {
                        r[op.b as usize] = TAG_UNINIT;
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
                        fast_stack.regs[new_base] = arg0;
                        let regs_n = cur_reg_count as usize;
                        if regs_n > 1 {
                            fast_stack.regs[new_base + 1..new_base + regs_n].fill(TAG_UNINIT);
                        }
                        cur_has_heap = false;
                        base = new_base;
                        ip = 0;
                        work += 1;
                        if work >= YIELD_AFTER_STEPS {
                            work = 0;
                            return materialize_deopt(
                                code,
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
                                YIELD_AFTER_STEPS as u64,
                            );
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
                                return materialize_deopt(
                                    code,
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
                                    YIELD_AFTER_STEPS as u64,
                                );
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
                    let arg0 = r[op.b as usize];
                    let arg1 = r[op.c as usize];
                    if arg0 < TAG_UNINIT
                        && arg1 < TAG_UNINIT
                        && fast_stack.frames.len() < max_fast_frames
                    {
                        r[op.b as usize] = TAG_UNINIT;
                        r[op.c as usize] = TAG_UNINIT;
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
                        fast_stack.regs[new_base] = arg0;
                        fast_stack.regs[new_base + 1] = arg1;
                        let regs_n = cur_reg_count as usize;
                        if regs_n > 2 {
                            fast_stack.regs[new_base + 2..new_base + regs_n].fill(TAG_UNINIT);
                        }
                        cur_has_heap = false;
                        base = new_base;
                        ip = 0;
                        work += 1;
                        if work >= YIELD_AFTER_STEPS {
                            work = 0;
                            return materialize_deopt(
                                code,
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
                                YIELD_AFTER_STEPS as u64,
                            );
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
                                return materialize_deopt(
                                    code,
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
                                    YIELD_AFTER_STEPS as u64,
                                );
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
                                return materialize_deopt(
                                    code,
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
                                    YIELD_AFTER_STEPS as u64,
                                );
                            }
                            continue 'frame_loop;
                        }
                        CallSetup::EmptyDone => {
                            work += 1;
                            if work >= YIELD_AFTER_STEPS {
                                work = 0;
                                return materialize_deopt(
                                    code,
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
                                    YIELD_AFTER_STEPS as u64,
                                );
                            }
                            ip += 1;
                            continue 'frame_loop;
                        }
                        CallSetup::Bail => break 'frame_loop,
                    }
                }
                Op::RetIntImm | Op::ReturnReg | Op::ReturnNull => {
                    let ret_w = match op.op {
                        Op::RetIntImm => box_i32(op.imm),
                        Op::ReturnNull => TAG_NULL,
                        Op::ReturnReg => {
                            let rw = r[op.a as usize];
                            if rw == TAG_UNINIT {
                                break 'frame_loop;
                            }
                            if rw >= TAG_HEAP {
                                let is_ref = heap
                                    .get(rw as u32 as usize)
                                    .and_then(Option::as_ref)
                                    .is_none_or(Value::is_reference);
                                if is_ref {
                                    break 'frame_loop;
                                }
                            }
                            rw
                        }
                        _ => unreachable!(),
                    };

                    if let Some(mask) = cur_fn.return_mask
                        && !return_mask_fits(mask, ret_w, heap, unit_consts)
                    {
                        break 'frame_loop;
                    }

                    // Returning from a nested FastFrame?
                    if let Some(caller_fast) = fast_stack.frames.pop() {
                        if ret_w >= TAG_HEAP && op.op == Op::ReturnReg {
                            r[op.a as usize] = TAG_UNINIT;
                        }
                        if cur_has_heap {
                            fast_stack.release_frame_heap(base, cur_reg_count as usize);
                        }
                        let caller_base = caller_fast.base as usize;
                        let caller_dst = caller_fast.return_dst as usize;
                        let mut caller_has_heap = caller_fast.has_heap;

                        let old_dst = fast_stack.regs[caller_base + caller_dst];
                        if old_dst >= TAG_HEAP {
                            fast_stack.free_heap_slot(old_dst as u32);
                        }
                        fast_stack.regs[caller_base + caller_dst] = ret_w;
                        if ret_w >= TAG_HEAP {
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
                    if ret_w >= TAG_HEAP && op.op == Op::ReturnReg {
                        r[op.a as usize] = TAG_UNINIT;
                    }
                    let Some(ret_value) = fast_stack.take_value(ret_w, unit_consts) else {
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
                // EXACT DEOPTIMIZATION
                // =============================================================
                Op::Deopt => {
                    break 'frame_loop;
                }
            }
        }
    }

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
        false,
        work_io,
        executed_io,
        work,
        (work.saturating_sub(work_start) as u64).max(1),
    )
}

/// Materialize the entry frame `F0` and every active nested [`FastFrame`]
/// back into `Vm::registers` and `Vm::call_stack` at the exact bytecode IP.
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
    }
}
