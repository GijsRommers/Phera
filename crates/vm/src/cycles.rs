//! The cycle collector: `gc_collect_cycles()` and its automatic runs.
//!
//! ```php
//! class Node { public $other; function __destruct() { echo "freed\n"; } }
//! $a = new Node; $b = new Node;
//! $a->other = $b; $b->other = $a;
//! unset($a, $b);                   // still alive: each holds the other
//! var_dump(gc_collect_cycles());   // freed, freed, int(2)
//! ```
//!
//! `phera_value::find_garbage` finds what only garbage holds (see there for
//! the algorithm). A run then does what PHP's collector does:
//!
//! 1. It calls the destructor of each garbage object that still has one
//!    due, in the order of their ids. A destructor may store `$this` (or
//!    anything it reaches) somewhere live again, so after destructors the
//!    search is repeated; PHP counts that as a second run.
//! 2. It breaks what is still garbage apart: it empties the objects'
//!    properties and the PHP references, and Rust's reference counting
//!    frees the rest. Nothing freed this way has a destructor left.
//!
//! `gc_collect_cycles()` returns the number of objects and arrays freed.
//!
//! Automatic runs: PHP starts a run when 10,000 values that may be part of
//! a cycle have been released. Phera counts object allocations instead:
//! after 10,000 of them it asks for a run at the next instruction boundary
//! (through the same flag as due destructors). A run costs time in
//! proportion to the live objects and what they hold; when it frees fewer
//! than 100 values the threshold grows by 10,000, as PHP's does, and the next
//! run never comes before twice as many allocations as there are live objects, so
//! that a program with a large live heap and no cycles pays a constant
//! amount per allocation for the walks.

use phera_extension_api::{NativeError, NativeResult};
use phera_value::{ArrayKey, Destruction, Garbage, PhpArray, Value, find_garbage};

use crate::Vm;

/// Allocations between automatic runs, and the step by which that grows.
const THRESHOLD_STEP: usize = 10_000;
/// The largest threshold, as in PHP.
const THRESHOLD_MAX: usize = 1_000_000_000;
/// A run that frees fewer values than this makes the next one wait longer.
const PRODUCTIVE_RUN: usize = 100;

/// Allocations left before the next automatic run.
pub(crate) struct Countdown(usize);

impl Countdown {
    pub(crate) fn new(allocations: usize) -> Self {
        Self(allocations)
    }

    /// Counts one allocation; `true` when the run is due. After that it
    /// waits until the collector sets a new count.
    #[inline]
    pub(crate) fn tick(&mut self) -> bool {
        match self.0 {
            0 => false,
            1 => {
                self.0 = 0;
                true
            }
            left => {
                self.0 = left - 1;
                false
            }
        }
    }
}

impl Default for Countdown {
    fn default() -> Self {
        Self(THRESHOLD_STEP)
    }
}

/// The `gc_*` functions.
#[derive(Clone, Copy)]
pub(crate) enum GcFunction {
    CollectCycles,
    Enable,
    Disable,
    Enabled,
    Status,
    MemCaches,
}

impl GcFunction {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::CollectCycles => "gc_collect_cycles",
            Self::Enable => "gc_enable",
            Self::Disable => "gc_disable",
            Self::Enabled => "gc_enabled",
            Self::Status => "gc_status",
            Self::MemCaches => "gc_mem_caches",
        }
    }

    pub(crate) fn resolve(name: &str) -> Option<Self> {
        Some(match name {
            "gc_collect_cycles" => Self::CollectCycles,
            "gc_enable" => Self::Enable,
            "gc_disable" => Self::Disable,
            "gc_enabled" => Self::Enabled,
            "gc_status" => Self::Status,
            "gc_mem_caches" => Self::MemCaches,
            _ => return None,
        })
    }
}

/// The collector's settings and counters for one request.
pub(crate) struct CycleCollector {
    enabled: bool,
    /// Allocations after which the next automatic run starts.
    threshold: usize,
    runs: i64,
    collected: i64,
}

impl Default for CycleCollector {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: THRESHOLD_STEP,
            runs: 0,
            collected: 0,
        }
    }
}

impl Vm {
    pub(crate) async fn gc_intrinsic(
        &mut self,
        function: GcFunction,
        mut arguments: Vec<Value>,
        origin: crate::calls::callback_binding::ArgumentOrigin,
    ) -> NativeResult {
        // None of them takes an argument: "gc_enabled() expects exactly 0
        // arguments, 1 given".
        self.with_argument_origin(origin, |vm| {
            phera_extension_api::prepare_call(function.name(), &[], &mut arguments, vm)
        })?;
        Ok(match function {
            GcFunction::CollectCycles => Value::Int(self.collect_cycles().await? as i64),
            GcFunction::Enable => {
                self.cycles.enabled = true;
                Value::Null
            }
            GcFunction::Disable => {
                self.cycles.enabled = false;
                Value::Null
            }
            GcFunction::Enabled => Value::Bool(self.cycles.enabled),
            GcFunction::Status => self.gc_status(),
            GcFunction::MemCaches => Value::Int(0),
        })
    }

    /// Runs the collector when enough objects were allocated since the
    /// last run; called with the destructor check.
    pub(crate) async fn collect_cycles_if_due(&mut self) -> Result<(), NativeError> {
        if !self.classes.take_collection_request() {
            return Ok(());
        }
        if self.cycles.enabled {
            let freed = self.collect_cycles().await?;
            self.cycles.threshold = if freed < PRODUCTIVE_RUN {
                (self.cycles.threshold + THRESHOLD_STEP).min(THRESHOLD_MAX)
            } else {
                THRESHOLD_STEP
            };
        }
        // A run walks every live object, so the next one waits for at least
        // twice as many allocations as there are live objects: a large heap
        // that holds no cycles costs a constant amount per allocation, not a
        // walk every 10,000.
        let live = self.object_ids.live();
        self.classes
            .request_collection_after(self.cycles.threshold.max(live.saturating_mul(2)));
        Ok(())
    }

    /// One run; returns the number of objects and arrays freed.
    pub(crate) async fn collect_cycles(&mut self) -> Result<usize, NativeError> {
        let mut garbage = find_garbage(self.classes.live_objects());
        let due: Vec<_> = garbage
            .objects
            .iter()
            .filter(|object| object.destruction() == Destruction::Due)
            .cloned()
            .collect();
        if !due.is_empty() {
            drop(garbage);
            for object in due {
                object.mark_destructed();
                self.destruct(object).await?;
                self.run_destructors().await?;
            }
            self.cycles.runs += 1;
            garbage = find_garbage(self.classes.live_objects());
        }
        let freed = garbage.objects.len() + garbage.arrays.len();
        break_apart(garbage);
        self.cycles.runs += 1;
        self.cycles.collected += freed as i64;
        Ok(freed)
    }

    /// `gc_status()`. Phera keeps no root buffer, so `roots` is 0, and it
    /// does not time its runs.
    fn gc_status(&self) -> Value {
        let mut status = PhpArray::new();
        let fields = [
            ("running", Value::Bool(false)),
            ("protected", Value::Bool(false)),
            ("full", Value::Bool(false)),
            ("runs", Value::Int(self.cycles.runs)),
            ("collected", Value::Int(self.cycles.collected)),
            ("threshold", Value::Int(self.cycles.threshold as i64 + 1)),
            ("buffer_size", Value::Int(16_384)),
            ("roots", Value::Int(0)),
            ("application_time", Value::Float(0.0)),
            ("collector_time", Value::Float(0.0)),
            ("destructor_time", Value::Float(0.0)),
            ("free_time", Value::Float(0.0)),
        ];
        for (name, value) in fields {
            status.insert(ArrayKey::str(name), value);
        }
        Value::from(status)
    }
}

/// Empties what holds the garbage together; reference counting frees the
/// rest. The values are dropped after all of them are emptied, so no
/// object is freed while another is being emptied.
fn break_apart(garbage: Garbage) {
    let mut contents = Vec::new();
    for object in &garbage.objects {
        if let Some(properties) = object.update(std::mem::take) {
            contents.push(properties);
        }
    }
    let mut values = Vec::new();
    for cell in &garbage.references {
        if let Ok(mut value) = cell.try_borrow_mut() {
            values.push(std::mem::take(&mut *value));
        }
    }
    drop(garbage);
    drop(contents);
    drop(values);
}
