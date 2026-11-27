//! Zero-drop `Copy` 8-byte NaN-boxed register stack + out-of-line heap side table.

use phera_value::Value;

use crate::calls::registers::Registers;
use crate::code_table::FunctionIndex;
use crate::fast_tier::{
    TAG_BOOL, TAG_CONST, TAG_HEAP, TAG_INT, TAG_MASK, TAG_NULL, TAG_UNINIT, box_bool, box_f64,
    box_int_checked, is_f64, unbox_f64, unbox_int,
};

/// One active nested caller frame inside `FastVm` (above the entry frame already
/// on `CallStack`). 24 bytes, `Copy`, zero heap allocation.
#[derive(Clone, Copy)]
pub(crate) struct FastFrame {
    pub(crate) function: FunctionIndex,
    pub(crate) base: u32,
    pub(crate) reg_count: u8,
    pub(crate) has_heap: bool,
    pub(crate) return_dst: u8,
    pub(crate) return_fast_ip: u16,
    pub(crate) return_bc_ip: u32,
}

/// Reusable per-VM Tier-2 register and frame storage. Always empty outside
/// `FastVm::run_fast`, so forks and fiber switches pay zero cost.
pub(crate) struct FastStack {
    pub(crate) regs: Vec<u64>,
    pub(crate) heap: Vec<Option<Value>>,
    pub(crate) free_heap: Vec<u32>,
    pub(crate) frames: Vec<FastFrame>,
}

impl FastStack {
    pub(crate) fn new() -> Self {
        Self {
            regs: Vec::new(),
            heap: Vec::new(),
            free_heap: Vec::new(),
            frames: Vec::new(),
        }
    }

    /// Ensure `[base..base + 256]` is addressable so fixed-array indexing
    /// needs zero bounds checks inside the hot dispatch loop.
    #[inline(always)]
    pub(crate) fn ensure_window(&mut self, base: usize) {
        let needed = base + 256;
        if self.regs.len() < needed {
            self.grow_regs(needed);
        }
    }

    #[cold]
    #[inline(never)]
    fn grow_regs(&mut self, needed: usize) {
        let new_len = (needed + 1024).max(self.regs.len() * 2).max(4096);
        self.regs.resize(new_len, TAG_UNINIT);
    }

    #[inline(always)]
    pub(crate) fn alloc_heap(&mut self, value: Value) -> u32 {
        if let Some(slot) = self.free_heap.pop() {
            self.heap[slot as usize] = Some(value);
            slot
        } else {
            let slot = self.heap.len() as u32;
            self.heap.push(Some(value));
            slot
        }
    }

    #[inline(always)]
    pub(crate) fn free_heap_slot(&mut self, idx: u32) {
        if let Some(entry) = self.heap.get_mut(idx as usize) {
            *entry = None;
            self.free_heap.push(idx);
        }
    }

    #[inline(always)]
    pub(crate) fn take_heap_slot(&mut self, idx: u32) -> Option<Value> {
        let val = self.heap.get_mut(idx as usize).and_then(Option::take);
        if val.is_some() {
            self.free_heap.push(idx);
        }
        val
    }

    /// Clear all state after leaving `FastVm` (return or deopt).
    #[inline]
    pub(crate) fn reset(&mut self) {
        self.frames.clear();
        if !self.heap.is_empty() {
            self.heap.clear();
            self.free_heap.clear();
        }
    }

    /// Load `reg_count` registers from `Registers[vm_base..]` into
    /// `FastStack.regs[0..reg_count]` by moving ownership out of `Registers`.
    /// Returns whether any loaded register holds a heap value.
    #[inline]
    pub(crate) fn load_entry_from_vm(
        &mut self,
        registers: &mut Registers,
        vm_base: usize,
        reg_count: usize,
    ) -> bool {
        self.ensure_window(0);
        let mut has_heap = false;
        for r in 0..reg_count {
            self.regs[r] = match registers.take_defined(vm_base + r) {
                None => TAG_UNINIT,
                Some(Value::Null) => TAG_NULL,
                Some(Value::Bool(b)) => box_bool(b),
                Some(Value::Int(i)) => match box_int_checked(i) {
                    Some(w) => w,
                    None => {
                        let idx = self.alloc_heap(Value::Int(i));
                        has_heap = true;
                        TAG_HEAP | u64::from(idx)
                    }
                },
                Some(Value::Float(f)) => box_f64(f),
                Some(other) => {
                    let idx = self.alloc_heap(other);
                    has_heap = true;
                    TAG_HEAP | u64::from(idx)
                }
            };
        }
        has_heap
    }

    /// Write `reg_count` registers from `FastStack.regs[fast_base..]` back into
    /// `Registers[vm_base..]`, consuming any heap handles.
    #[inline]
    pub(crate) fn store_frame_to_vm(
        &mut self,
        registers: &mut Registers,
        constants: &[Value],
        fast_base: usize,
        vm_base: usize,
        reg_count: usize,
    ) {
        for r in 0..reg_count {
            let w = self.regs[fast_base + r];
            self.regs[fast_base + r] = TAG_UNINIT;
            if is_f64(w) {
                registers.put(vm_base + r, Value::Float(unbox_f64(w)));
                continue;
            }
            match w & TAG_MASK {
                TAG_UNINIT => {}
                TAG_NULL => registers.put(vm_base + r, Value::Null),
                TAG_BOOL => registers.put(vm_base + r, Value::Bool((w & 1) != 0)),
                TAG_INT => registers.put(vm_base + r, Value::Int(unbox_int(w))),
                TAG_CONST => {
                    if let Some(val) = constants.get(w as u32 as usize) {
                        registers.put(vm_base + r, val.clone());
                    }
                }
                TAG_HEAP => {
                    if let Some(value) = self.take_heap_slot(w as u32) {
                        registers.put(vm_base + r, value);
                    }
                }
                _ => {}
            }
        }
    }

    /// Release any remaining heap slots owned by `[fast_base..fast_base + reg_count]`.
    #[inline]
    pub(crate) fn release_frame_heap(&mut self, fast_base: usize, reg_count: usize) {
        for r in 0..reg_count {
            let w = self.regs[fast_base + r];
            if w >= TAG_HEAP {
                self.free_heap_slot(w as u32);
                self.regs[fast_base + r] = TAG_UNINIT;
            }
        }
    }

    /// Materialize one NaN-boxed `u64` into an owned `Value`, taking from `heap`
    /// when `TAG_HEAP` (for frame returns).
    #[inline]
    pub(crate) fn take_value(&mut self, w: u64, constants: &[Value]) -> Option<Value> {
        if is_f64(w) {
            return Some(Value::Float(unbox_f64(w)));
        }
        match w & TAG_MASK {
            TAG_NULL => Some(Value::Null),
            TAG_BOOL => Some(Value::Bool((w & 1) != 0)),
            TAG_INT => Some(Value::Int(unbox_int(w))),
            TAG_CONST => constants.get(w as u32 as usize).cloned(),
            TAG_HEAP => self.take_heap_slot(w as u32),
            _ => None,
        }
    }
}
