//! A frame is five numbers; what only some frames need lives beside it.
//!
//! During `f(1, g(2))`, after `g` has been entered:
//!
//! ```text
//! frames (Vec<Frame>):   [ {main} base 0 ]  [ g base 9, destination r6 ]
//! registers:             [ main's locals and temporaries | g: 2, g's locals ]
//!                          0                                9
//! side table (by depth):   0: dynamic names of {main}       1: (unused)
//! ```
//!
//! `f`'s first argument is already in main's temporaries; when `g` returns, its
//! result goes to main's r6 and `g`'s registers are emptied again. Pushing
//! and popping a frame copies sixteen bytes. A frame that uses `foreach`,
//! `$$name` or `func_get_args()` keeps that state in `FrameExtras` at its
//! depth, and only such frames touch it. A method's `$this` and a closure
//! are in its window (see `window_context`); `FrameView` reads a frame.

use std::rc::Rc;

use phera_bytecode::Reg;
use phera_value::PhpArray;

use crate::AccessPath;
use crate::calls::prepared_call::CallKind;
use crate::code_table::FunctionIndex;

pub(crate) use crate::calls::frame_view::FrameView;

/// Foreach either owns a value snapshot or follows a live reference path.
pub(crate) struct IteratorState {
    /// By-value foreach keeps this owner to separate subsequent array writes.
    pub(crate) snapshot: Option<Rc<PhpArray>>,
    /// Objects keep an independent cursor for each active traversal.
    pub(crate) traversal: Option<Box<dyn phera_extension_api::traversal::Traversal>>,
    pub(crate) property_hooks:
        Option<Box<crate::hooked_property_iteration::HookedPropertyIterator>>,
    /// By-reference foreach resolves this place again as the array changes.
    pub(crate) source: Option<AccessPath>,
    /// The array storage position, distinct from a PHP integer key.
    pub(crate) position: Option<usize>,
    /// Distinguishes an untouched iterator from an exhausted one.
    pub(crate) started: bool,
}

/// One running PHP function: which code, where its registers start, where it
/// continues, and where its result goes.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    pub(crate) function: FunctionIndex,
    base: u32,
    ip: u32,
    /// The caller's register that receives the result.
    destination: Reg,
    flags: FrameFlags,
}

/// Small facts about a frame, one bit each.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct FrameFlags(u16);

impl FrameFlags {
    /// The frame's side-table entry holds something and is emptied on return.
    const EXTRAS: u16 = 1;
    /// Native code called this function: its result goes back to Rust, not
    /// to a register.
    const CALLBACK: u16 = 1 << 1;
    /// Bits 2 and 3 hold the frame's [`CallKind`], which says what its
    /// window keeps below the first register.
    const KIND_SHIFT: u16 = 2;
    const KIND_MASK: u16 = 0b11 << Self::KIND_SHIFT;
    /// A setter entered for its effects has no result destination or continuation.
    const DISCARD_RESULT: u16 = 1 << 4;
    /// The top eight bits hold how many arguments the call passed, when it
    /// passed fewer than the function's parameters (at most 64, see
    /// `FunctionShape::Plain`), so that a call with defaults needs no
    /// side-table entry. All ones means "one per parameter".
    const PASSED_SHIFT: u16 = 8;
    const PASSED_UNKNOWN: u16 = 0xff;

    #[inline]
    fn of_kind(kind: CallKind) -> u16 {
        (kind as u16) << Self::KIND_SHIFT
    }
}

const _: () = assert!(std::mem::size_of::<Frame>() == 16);

impl Frame {
    /// A frame about to run `function` from its first instruction.
    #[inline]
    pub(crate) fn new(function: FunctionIndex, base: usize, destination: Option<Reg>) -> Self {
        Self::of_kind(function, CallKind::Function, base, destination)
    }

    /// A frame of `kind` whose first parameter is at register `base`.
    #[inline]
    pub(crate) fn of_kind(
        function: FunctionIndex,
        kind: CallKind,
        base: usize,
        destination: Option<Reg>,
    ) -> Self {
        let passed = FrameFlags::PASSED_UNKNOWN << FrameFlags::PASSED_SHIFT;
        let mut flags = FrameFlags(FrameFlags::of_kind(kind) | passed);
        if destination.is_none() {
            flags.0 |= FrameFlags::CALLBACK;
        }
        Self {
            function,
            base: base as u32,
            ip: 0,
            destination: destination.unwrap_or(Reg(0)),
            flags,
        }
    }

    /// Where this frame's registers start in the request's register stack.
    #[inline]
    pub(crate) fn base(&self) -> usize {
        self.base as usize
    }

    /// The same frame, recording that its call passed only `passed`
    /// arguments (fewer than 64).
    #[inline]
    pub(crate) fn with_passed(mut self, passed: usize) -> Self {
        let mask = FrameFlags::PASSED_UNKNOWN << FrameFlags::PASSED_SHIFT;
        self.flags.0 = (self.flags.0 & !mask) | ((passed as u16) << FrameFlags::PASSED_SHIFT);
        self
    }

    /// How many arguments the call passed, when it recorded that in the
    /// frame (fewer than the parameters, through the quick entry).
    #[inline]
    pub(crate) fn passed(&self) -> Option<usize> {
        let passed = self.flags.0 >> FrameFlags::PASSED_SHIFT;
        (passed != FrameFlags::PASSED_UNKNOWN).then_some(passed as usize)
    }

    /// What kind of code the frame runs.
    #[inline]
    pub(crate) fn kind(&self) -> CallKind {
        match (self.flags.0 & FrameFlags::KIND_MASK) >> FrameFlags::KIND_SHIFT {
            0 => CallKind::Function,
            1 => CallKind::Method,
            2 => CallKind::Constructor,
            _ => CallKind::Closure,
        }
    }

    /// Where the frame's window starts, below its first register when it
    /// keeps `$this` or a closure there. Returning empties from here.
    #[inline]
    pub(crate) fn start(&self) -> usize {
        self.base() - self.kind().prefix()
    }

    /// The next instruction to run.
    #[inline]
    pub(crate) fn ip(&self) -> usize {
        self.ip as usize
    }

    #[inline]
    pub(crate) fn set_ip(&mut self, ip: usize) {
        self.ip = ip as u32;
    }

    /// The same frame with its registers starting at `base` instead.
    pub(crate) fn moved_to(self, base: usize) -> Self {
        Self {
            base: base as u32,
            ..self
        }
    }

    /// The caller's register for the result; `None` when native code called.
    #[inline]
    pub(crate) fn destination(&self) -> Option<Reg> {
        (self.flags.0 & FrameFlags::CALLBACK == 0).then_some(self.destination)
    }

    /// Whether the side table holds state for this frame.
    #[inline]
    pub(crate) fn has_extras(&self) -> bool {
        self.flags.0 & FrameFlags::EXTRAS != 0
    }

    pub(crate) fn mark_extras(&mut self) {
        self.flags.0 |= FrameFlags::EXTRAS;
    }

    pub(crate) fn discard_result(&mut self) {
        self.flags.0 |= FrameFlags::DISCARD_RESULT;
    }

    pub(super) fn discards_result(&self) -> bool {
        self.flags.0 & FrameFlags::DISCARD_RESULT != 0
    }
}
