//! Zero-drop `Copy` raw `(tags: Vec<u8>, bits: Vec<u64>)` register stack +
//! out-of-line heap side table + reusable raw-`i64` recursion buffers.

use phera_value::Value;

use crate::calls::registers::Registers;
use crate::code_table::FunctionIndex;
use crate::fast_tier::int_rec::RecFrame;
use crate::fast_tier::{TAG_BOOL, TAG_CONST, TAG_FLOAT, TAG_HEAP, TAG_INT, TAG_NULL, TAG_UNINIT};

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

/// Reusable per-VM Tier-2 register and frame storage. Lazily boxed inside
/// `Vm::fast_stack` (`None` on every HTTP request and non-Tier-2 script).
pub(crate) struct FastStack {
    pub(crate) tags: Vec<u8>,
    pub(crate) bits: Vec<u64>,
    pub(crate) heap: Vec<Option<Value>>,
    pub(crate) free_heap: Vec<u32>,
    pub(crate) frames: Vec<FastFrame>,
    /// Reusable flat raw-`i64` register stack for `int_rec::run_int_rec`.
    pub(crate) rec_regs: Vec<i64>,
    /// Reusable compact 8-byte call frames for `int_rec::run_int_rec`.
    pub(crate) rec_frames: Vec<RecFrame>,
    /// Reusable unboxed `i64` scratch slice for `list_loop::try_run_list_loop`.
    pub(crate) list_buf_i64: Vec<i64>,
    /// Reusable unboxed `f64` scratch slice for `list_loop::try_run_list_loop`.
    pub(crate) list_buf_f64: Vec<f64>,
}

impl FastStack {
    pub(crate) fn new() -> Self {
        Self {
            tags: Vec::new(),
            bits: Vec::new(),
            heap: Vec::new(),
            free_heap: Vec::new(),
            frames: Vec::new(),
            rec_regs: Vec::new(),
            rec_frames: Vec::new(),
            list_buf_i64: Vec::new(),
            list_buf_f64: Vec::new(),
        }
    }

    /// Ensure `[base..base + 256]` is addressable so fixed-array indexing
    /// needs zero bounds checks inside the hot dispatch loop.
    #[inline(always)]
    pub(crate) fn ensure_window(&mut self, base: usize) {
        let needed = base + 256;
        if self.tags.len() < needed {
            self.grow_regs(needed);
        }
    }

    #[cold]
    #[inline(never)]
    fn grow_regs(&mut self, needed: usize) {
        let new_len = (needed + 1024).max(self.tags.len() * 2).max(4096);
        self.tags.resize(new_len, TAG_UNINIT);
        self.bits.resize(new_len, 0);
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
                    self.bits[r] = 0;
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
                    has_heap = true;
                    self.tags[r] = TAG_HEAP;
                    self.bits[r] = u64::from(idx);
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
        constants: &[Value],
        fast_base: usize,
        vm_base: usize,
        reg_count: usize,
    ) {
        for r in 0..reg_count {
            let tag = self.tags[fast_base + r];
            self.tags[fast_base + r] = TAG_UNINIT;
            match tag {
                TAG_UNINIT => {}
                TAG_NULL => registers.put(vm_base + r, Value::Null),
                TAG_BOOL => registers.put(vm_base + r, Value::Bool(self.bits[fast_base + r] != 0)),
                TAG_INT => registers.put(vm_base + r, Value::Int(self.bits[fast_base + r] as i64)),
                TAG_FLOAT => registers.put(
                    vm_base + r,
                    Value::Float(f64::from_bits(self.bits[fast_base + r])),
                ),
                TAG_CONST => {
                    if let Some(val) = constants.get(self.bits[fast_base + r] as usize) {
                        registers.put(vm_base + r, val.clone());
                    }
                }
                TAG_HEAP => {
                    let idx = self.bits[fast_base + r] as u32;
                    if let Some(value) = self.take_heap_slot(idx) {
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
            if self.tags[fast_base + r] == TAG_HEAP {
                let idx = self.bits[fast_base + r] as u32;
                self.free_heap_slot(idx);
                self.tags[fast_base + r] = TAG_UNINIT;
            }
        }
    }

    /// Materialize one `(tag, bit)` into an owned `Value`, taking from `heap`
    /// when `TAG_HEAP` (for frame returns).
    #[inline]
    pub(crate) fn take_value(&mut self, tag: u8, bit: u64, constants: &[Value]) -> Option<Value> {
        match tag {
            TAG_NULL => Some(Value::Null),
            TAG_BOOL => Some(Value::Bool(bit != 0)),
            TAG_INT => Some(Value::Int(bit as i64)),
            TAG_FLOAT => Some(Value::Float(f64::from_bits(bit))),
            TAG_CONST => constants.get(bit as usize).cloned(),
            TAG_HEAP => self.take_heap_slot(bit as u32),
            _ => None,
        }
    }
}
