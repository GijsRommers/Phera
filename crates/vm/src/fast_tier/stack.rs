//! Zero-drop `Copy` scalar register stack + out-of-line heap side table.

use phera_value::Value;

use crate::calls::registers::Registers;
use crate::code_table::FunctionIndex;
use crate::fast_tier::{TAG_BOOL, TAG_FLOAT, TAG_HEAP, TAG_INT, TAG_NULL, TAG_UNINIT};

/// One active frame inside `FastVm` (above the entry frame already on `CallStack`).
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
    pub(crate) tags: Vec<u8>,
    pub(crate) bits: Vec<u64>,
    pub(crate) heap: Vec<Option<Value>>,
    pub(crate) free_heap: Vec<u32>,
    pub(crate) frames: Vec<FastFrame>,
}

impl FastStack {
    pub(crate) fn new() -> Self {
        Self {
            tags: Vec::new(),
            bits: Vec::new(),
            heap: Vec::new(),
            free_heap: Vec::new(),
            frames: Vec::new(),
        }
    }

    /// Ensure `[base..base + 256]` is addressable so fixed-array indexing
    /// needs zero bounds checks inside the hot dispatch loop.
    #[inline]
    pub(crate) fn ensure_window(&mut self, base: usize) {
        let needed = base + 256;
        if self.tags.len() < needed {
            let new_len = (needed + 256).max(self.tags.len() * 2);
            self.tags.resize(new_len, TAG_UNINIT);
            self.bits.resize(new_len, 0);
        }
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
        let slot = idx as usize;
        if let Some(entry) = self.heap.get_mut(slot) {
            *entry = None;
            self.free_heap.push(idx);
        }
    }

    #[inline(always)]
    pub(crate) fn take_heap_slot(&mut self, idx: u32) -> Option<Value> {
        let slot = idx as usize;
        let val = self.heap.get_mut(slot).and_then(Option::take);
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
    /// `FastStack[0..reg_count]` by moving ownership out of `Registers`.
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
            match registers.take_defined(vm_base + r) {
                None => {
                    self.tags[r] = TAG_UNINIT;
                }
                Some(Value::Null) => {
                    self.tags[r] = TAG_NULL;
                }
                Some(Value::Bool(b)) => {
                    self.tags[r] = TAG_BOOL;
                    self.bits[r] = u64::from(b);
                }
                Some(Value::Int(i)) => {
                    self.tags[r] = TAG_INT;
                    self.bits[r] = i as u64;
                }
                Some(Value::Float(f)) => {
                    self.tags[r] = TAG_FLOAT;
                    self.bits[r] = f.to_bits();
                }
                Some(other) => {
                    let idx = self.alloc_heap(other);
                    self.tags[r] = TAG_HEAP;
                    self.bits[r] = u64::from(idx);
                    has_heap = true;
                }
            }
        }
        has_heap
    }

    /// Write `reg_count` registers from `FastStack[fast_base..]` back into
    /// `Registers[vm_base..]`, consuming any heap handles.
    #[inline]
    pub(crate) fn store_frame_to_vm(
        &mut self,
        registers: &mut Registers,
        fast_base: usize,
        vm_base: usize,
        reg_count: usize,
    ) {
        for r in 0..reg_count {
            let tag = self.tags[fast_base + r];
            let bit = self.bits[fast_base + r];
            match tag {
                TAG_UNINIT => {}
                TAG_NULL => registers.put(vm_base + r, Value::Null),
                TAG_BOOL => registers.put(vm_base + r, Value::Bool(bit != 0)),
                TAG_INT => registers.put(vm_base + r, Value::Int(bit as i64)),
                TAG_FLOAT => registers.put(vm_base + r, Value::Float(f64::from_bits(bit))),
                TAG_HEAP => {
                    if let Some(value) = self.take_heap_slot(bit as u32) {
                        registers.put(vm_base + r, value);
                    }
                }
                _ => {}
            }
            self.tags[fast_base + r] = TAG_UNINIT;
        }
    }

    /// Release any remaining heap slots owned by `[fast_base..fast_base + reg_count]`.
    #[inline]
    pub(crate) fn release_frame_heap(&mut self, fast_base: usize, reg_count: usize) {
        for r in 0..reg_count {
            if self.tags[fast_base + r] == TAG_HEAP {
                let idx = self.bits[fast_base + r] as u32;
                self.free_heap_slot(idx);
                self.tags[fast_base + r] = TAG_UNINIT;
            }
        }
    }

    /// Materialize one `(tag, bit)` pair into an owned `Value`, taking from
    /// `heap` when `TAG_HEAP` (for frame returns).
    #[inline]
    pub(crate) fn take_value(&mut self, tag: u8, bit: u64) -> Option<Value> {
        match tag {
            TAG_NULL => Some(Value::Null),
            TAG_BOOL => Some(Value::Bool(bit != 0)),
            TAG_INT => Some(Value::Int(bit as i64)),
            TAG_FLOAT => Some(Value::Float(f64::from_bits(bit))),
            TAG_HEAP => self.take_heap_slot(bit as u32),
            _ => None,
        }
    }
}
