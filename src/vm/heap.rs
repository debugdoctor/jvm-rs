//! Heap storage: `HeapValue` variants, `Heap` with mark-and-sweep GC, and
//! `GcStats` counters. Accessed through `Vm::heap` (behind a `Mutex`).
//!
//! # Reference model and compressed-reference plan
//!
//! Current: `Reference::Heap(usize)` — a plain index into `Heap.values`
//! (`Vec<Option<HeapValue>>`). Each reference is 8 bytes on 64-bit platforms.
//!
//! HotSpot compressed oops encode references as 32-bit offsets from a heap
//! base pointer, halving reference size when the heap fits in 32 GB. Migration
//! path for jvm-rs when RSS pressure warrants it:
//! 1. Replace `Reference::Heap(usize)` with `Reference::Heap(u32)`.
//! 2. Store a `base_addr: *mut u8` in `Heap`; decode as `base + offset * 8`.
//! 3. Cap max heap size at `u32::MAX * 8` bytes (~32 GB).
//! Deferred until heap profiling shows reference size is the bottleneck.

use std::collections::HashMap;

use super::types::{Reference, Value, VmError};

#[derive(Debug, Clone)]
pub(super) enum HeapValue {
    IntArray {
        values: Vec<i32>,
    },
    ReferenceArray {
        component_type: String,
        values: Vec<Reference>,
    },
    String(String),
    LongArray {
        values: Vec<i64>,
    },
    FloatArray {
        values: Vec<f32>,
    },
    DoubleArray {
        values: Vec<f64>,
    },
    Object {
        class_name: String,
        fields: Vec<Value>,
    },
    StringBuilder(std::string::String),
}

impl HeapValue {
    pub(super) fn kind_name(&self) -> &'static str {
        match self {
            Self::IntArray { .. } => "int-array",
            Self::LongArray { .. } => "long-array",
            Self::FloatArray { .. } => "float-array",
            Self::DoubleArray { .. } => "double-array",
            Self::ReferenceArray { .. } => "reference-array",
            Self::String(_) => "string",
            Self::Object { .. } => "object",
            Self::StringBuilder(_) => "string-builder",
        }
    }

    pub(super) fn heap_size(&self) -> usize {
        match self {
            Self::IntArray { values } => std::mem::size_of::<Vec<i32>>() + values.capacity() * 4,
            Self::LongArray { values } => std::mem::size_of::<Vec<i64>>() + values.capacity() * 8,
            Self::FloatArray { values } => std::mem::size_of::<Vec<f32>>() + values.capacity() * 4,
            Self::DoubleArray { values } => std::mem::size_of::<Vec<f64>>() + values.capacity() * 8,
            Self::ReferenceArray { values, .. } => {
                std::mem::size_of::<Vec<Reference>>() + values.capacity() * 8
            }
            Self::String(s) => std::mem::size_of::<String>() + s.capacity(),
            Self::Object { fields, .. } => {
                std::mem::size_of::<HashMap<String, Value>>() + fields.capacity() * 32
            }
            Self::StringBuilder(sb) => std::mem::size_of::<String>() + sb.capacity(),
        }
    }
}

/// Snapshot of garbage-collector counters for tooling / tests.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GcStats {
    /// Number of completed collections.
    pub collections: u64,
    /// Cumulative number of objects freed across all collections.
    pub freed: u64,
    /// Number of live heap slots after the most recent collection.
    pub live: usize,
    /// Total number of allocations observed since VM start.
    pub total_allocations: u64,
    /// Cumulative GC pause time in nanoseconds.
    pub pause_time_ns: u64,
    /// Cumulative bytes freed across all collections.
    pub freed_bytes: u64,
    /// Estimated total heap bytes currently in use (sum of live object sizes).
    pub total_heap_bytes: usize,
    /// Number of objects freed in the most recent collection.
    pub last_collection_freed: usize,
    /// Number of TLAB allocations (fast path).
    pub tlab_allocations: u64,
    /// Number of times TLAB was refilled.
    pub tlab_refills: u64,
}

#[derive(Debug, Clone)]
pub(super) struct Heap {
    pub(super) values: Vec<Option<HeapValue>>,
    /// Number of live objects (approximate, updated by GC).
    pub(super) live_count: usize,
    /// Number of allocations since last GC.
    pub(super) allocs_since_gc: usize,
    /// Allocation threshold that triggers collection. `usize::MAX` disables GC.
    pub(super) gc_threshold: usize,
    /// Cumulative GC statistics.
    pub(super) stats: GcStats,
    /// Thread-local allocation buffer: next free slot index.
    pub(super) tlab_top: usize,
    /// End of current TLAB (start of next TLAB will be here).
    pub(super) tlab_limit: usize,
    /// Default TLAB size in slots.
    pub(super) tlab_size: usize,
    /// Object ages for generational GC (index matches values slot).
    pub(super) ages: Vec<u8>,
    /// End of young generation (eden + survivor space).
    pub(super) young_end: usize,
    /// End of survivor space (eden_end < survivor_end < values.len()).
    pub(super) survivor_end: usize,
    /// Max age before promotion to old generation.
    pub(super) promotion_age: u8,
    /// Number of minor GCs performed.
    pub(super) minor_gc_count: u64,
    /// Number of major (full) GCs performed.
    pub(super) major_gc_count: u64,
    /// Remembered set: (source_slot, target_slot) pairs where source is old and target is young.
    /// Used by write barrier to track old->young references.
    pub(super) remembered_set: Vec<(usize, usize)>,
}

impl Default for Heap {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            live_count: 0,
            allocs_since_gc: 0,
            gc_threshold: 1024,
            stats: GcStats::default(),
            tlab_top: 0,
            tlab_limit: 0,
            tlab_size: 256,
            ages: Vec::new(),
            young_end: 0,
            survivor_end: 0,
            promotion_age: 4,
            minor_gc_count: 0,
            major_gc_count: 0,
            remembered_set: Vec::new(),
        }
    }
}

impl Heap {
    pub(super) fn allocate_int_array(&mut self, values: Vec<i32>) -> Reference {
        self.allocate(HeapValue::IntArray { values })
    }

    pub(super) fn allocate(&mut self, value: HeapValue) -> Reference {
        self.allocs_since_gc += 1;
        self.stats.total_allocations = self.stats.total_allocations.saturating_add(1);

        // TLAB bump allocation - fast path
        if self.tlab_top < self.tlab_limit {
            let slot = self.tlab_top;
            self.tlab_top += 1;
            self.stats.tlab_allocations += 1;
            while self.values.len() <= slot {
                self.values.push(None);
                self.ages.push(0);
            }
            self.values[slot] = Some(value);
            return Reference::Heap(slot);
        }

        // TLAB exhausted - allocate new TLAB at heap end
        self.refill_tlab();

        let slot = self.tlab_top;
        self.tlab_top += 1;
        self.stats.tlab_allocations += 1;
        while self.values.len() <= slot {
            self.values.push(None);
            self.ages.push(0);
        }
        self.values[slot] = Some(value);
        Reference::Heap(slot)
    }

    fn refill_tlab(&mut self) {
        self.stats.tlab_refills += 1;
        self.tlab_top = self.values.len();
        self.tlab_limit = self.values.len().saturating_add(self.tlab_size);
        self.values.resize(self.tlab_limit, None);
        self.ages.resize(self.tlab_limit, 0);
    }

    /// Write barrier: record old->young references for generational GC.
    /// Called when storing a reference into slot `source_slot` that points to `target_slot`.
    pub(super) fn write_barrier(&mut self, source_slot: usize, target_slot: usize) {
        // Only track if source is old (tenured) and target is young
        if source_slot >= self.survivor_end && target_slot < self.survivor_end {
            // Avoid duplicates by checking if already in set
            if !self.remembered_set.contains(&(source_slot, target_slot)) {
                self.remembered_set.push((source_slot, target_slot));
            }
        }
    }

    /// Clear the remembered set after GC.
    pub(super) fn clear_remembered_set(&mut self) {
        self.remembered_set.clear();
    }

    /// Get references from remembered set for minor GC tracing.
    pub(super) fn get_remembered_set_references(&self) -> Vec<usize> {
        self.remembered_set
            .iter()
            .map(|(_, target)| *target)
            .collect()
    }

    pub(super) fn allocate_string(&mut self, value: impl Into<String>) -> Reference {
        self.allocate(HeapValue::String(value.into()))
    }

    pub(super) fn allocate_reference_array(
        &mut self,
        component_type: impl Into<String>,
        values: Vec<Reference>,
    ) -> Reference {
        self.allocate(HeapValue::ReferenceArray {
            component_type: component_type.into(),
            values,
        })
    }

    pub(super) fn get(&self, reference: Reference) -> Result<&HeapValue, VmError> {
        match reference {
            Reference::Null => Err(VmError::NullReference),
            Reference::Heap(index) => self
                .values
                .get(index)
                .and_then(|v| v.as_ref())
                .ok_or(VmError::InvalidHeapReference { reference: index }),
        }
    }

    /// Returns the number of heap slots currently in use.
    #[allow(dead_code)]
    pub(super) fn len(&self) -> usize {
        self.values.iter().filter(|v| v.is_some()).count()
    }

    pub(super) fn array_length(&self, reference: Reference) -> Result<usize, VmError> {
        match self.get(reference)? {
            HeapValue::IntArray { values } => Ok(values.len()),
            HeapValue::LongArray { values } => Ok(values.len()),
            HeapValue::FloatArray { values } => Ok(values.len()),
            HeapValue::DoubleArray { values } => Ok(values.len()),
            HeapValue::ReferenceArray { values, .. } => Ok(values.len()),
            value => Err(VmError::InvalidHeapValue {
                expected: "array",
                actual: value.kind_name(),
            }),
        }
    }

    pub(super) fn load_int_array_element(
        &self,
        reference: Reference,
        index: i32,
    ) -> Result<i32, VmError> {
        let values = match self.get(reference)? {
            HeapValue::IntArray { values } => values,
            value => {
                return Err(VmError::InvalidHeapValue {
                    expected: "int-array",
                    actual: value.kind_name(),
                });
            }
        };

        let index = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len: values.len(),
        })?;

        values
            .get(index)
            .copied()
            .ok_or(VmError::ArrayIndexOutOfBounds {
                index: index as i32,
                len: values.len(),
            })
    }

    pub(super) fn load_reference_array_element(
        &self,
        reference: Reference,
        index: i32,
    ) -> Result<Reference, VmError> {
        let values = match self.get(reference)? {
            HeapValue::ReferenceArray { values, .. } => values,
            value => {
                return Err(VmError::InvalidHeapValue {
                    expected: "reference-array",
                    actual: value.kind_name(),
                });
            }
        };

        let index = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len: values.len(),
        })?;

        values
            .get(index)
            .copied()
            .ok_or(VmError::ArrayIndexOutOfBounds {
                index: index as i32,
                len: values.len(),
            })
    }

    pub(super) fn store_reference_array_element(
        &mut self,
        reference: Reference,
        index: i32,
        value: Reference,
    ) -> Result<(), VmError> {
        let array_slot = match reference {
            Reference::Heap(i) => i,
            Reference::Null => return Err(VmError::NullReference),
        };

        // Extract target slot for write barrier before mutable borrow
        let target_slot_for_barrier = if let Reference::Heap(slot) = value {
            Some(slot)
        } else {
            None
        };

        // First check if it's a ReferenceArray
        let is_ref_array = { matches!(self.get(reference), Ok(HeapValue::ReferenceArray { .. })) };

        if !is_ref_array {
            return Err(VmError::InvalidHeapValue {
                expected: "reference-array",
                actual: "non-reference-array",
            });
        }

        // Do write barrier first (needs mutable access before the next borrow)
        if let Some(target_slot) = target_slot_for_barrier {
            self.write_barrier(array_slot, target_slot);
        }

        // Now do the store
        let values = match self.get_mut(reference)? {
            HeapValue::ReferenceArray { values, .. } => values,
            _ => unreachable!(),
        };

        let index = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len: values.len(),
        })?;

        let len = values.len();
        let slot = values
            .get_mut(index)
            .ok_or(VmError::ArrayIndexOutOfBounds {
                index: index as i32,
                len,
            })?;

        *slot = value;
        Ok(())
    }

    pub(super) fn store_int_array_element(
        &mut self,
        reference: Reference,
        index: i32,
        value: i32,
    ) -> Result<(), VmError> {
        let values = match self.get_mut(reference)? {
            HeapValue::IntArray { values } => values,
            value => {
                return Err(VmError::InvalidHeapValue {
                    expected: "int-array",
                    actual: value.kind_name(),
                });
            }
        };

        let index = usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds {
            index,
            len: values.len(),
        })?;

        let len = values.len();
        let slot = values
            .get_mut(index)
            .ok_or(VmError::ArrayIndexOutOfBounds {
                index: index as i32,
                len,
            })?;
        *slot = value;
        Ok(())
    }

    /// Generic typed array element load.
    pub(super) fn load_typed_array_element(
        &self,
        reference: Reference,
        index: i32,
    ) -> Result<Value, VmError> {
        let heap_val = self.get(reference)?;
        let (value, len) = match heap_val {
            HeapValue::LongArray { values } => {
                let i = Self::check_array_index(index, values.len())?;
                (Value::Long(values[i]), values.len())
            }
            HeapValue::FloatArray { values } => {
                let i = Self::check_array_index(index, values.len())?;
                (Value::Float(values[i]), values.len())
            }
            HeapValue::DoubleArray { values } => {
                let i = Self::check_array_index(index, values.len())?;
                (Value::Double(values[i]), values.len())
            }
            _ => {
                return Err(VmError::InvalidHeapValue {
                    expected: "typed-array",
                    actual: heap_val.kind_name(),
                });
            }
        };
        let _ = len;
        Ok(value)
    }

    /// Generic typed array element store.
    pub(super) fn store_typed_array_element(
        &mut self,
        reference: Reference,
        index: i32,
        value: Value,
    ) -> Result<(), VmError> {
        let heap_val = self.get_mut(reference)?;
        match (heap_val, value) {
            (HeapValue::LongArray { values }, Value::Long(v)) => {
                let i = Self::check_array_index(index, values.len())?;
                values[i] = v;
            }
            (HeapValue::FloatArray { values }, Value::Float(v)) => {
                let i = Self::check_array_index(index, values.len())?;
                values[i] = v;
            }
            (HeapValue::DoubleArray { values }, Value::Double(v)) => {
                let i = Self::check_array_index(index, values.len())?;
                values[i] = v;
            }
            _ => {
                return Err(VmError::TypeMismatch {
                    expected: "matching array/value type",
                    actual: "mismatched",
                });
            }
        }
        Ok(())
    }

    pub(super) fn check_array_index(index: i32, len: usize) -> Result<usize, VmError> {
        let i =
            usize::try_from(index).map_err(|_| VmError::ArrayIndexOutOfBounds { index, len })?;
        if i >= len {
            return Err(VmError::ArrayIndexOutOfBounds { index, len });
        }
        Ok(i)
    }

    pub(super) fn get_mut(&mut self, reference: Reference) -> Result<&mut HeapValue, VmError> {
        match reference {
            Reference::Null => Err(VmError::NullReference),
            Reference::Heap(index) => self
                .values
                .get_mut(index)
                .and_then(|v| v.as_mut())
                .ok_or(VmError::InvalidHeapReference { reference: index }),
        }
    }

    /// Mark-and-sweep garbage collection.
    ///
    /// `roots` must contain every `Reference` reachable from the thread stacks,
    /// static fields, and any other GC roots.
    ///
    /// This implements a generational GC:
    /// - Minor GC: collects young generation (eden + survivor spaces), survivors age
    /// - Major GC: collects entire heap when tenured fills
    pub(super) fn gc(&mut self, roots: &[Reference]) {
        let start = std::time::Instant::now();

        // Determine if this should be a minor or major GC
        let is_minor = self.values.len() < self.survivor_end * 2;

        if is_minor {
            self.minor_gc_internal(roots);
        } else {
            self.major_gc_internal(roots);
        }

        let pause_ns = start.elapsed().as_nanos() as u64;

        // Calculate total heap bytes from live objects.
        let total_heap_bytes = self
            .values
            .iter()
            .filter_map(|v| v.as_ref())
            .map(|v| v.heap_size())
            .sum();

        self.stats.collections = self.stats.collections.saturating_add(1);
        self.stats.pause_time_ns = self.stats.pause_time_ns.saturating_add(pause_ns);
        self.stats.total_heap_bytes = total_heap_bytes;

        // Reset TLAB after GC - start fresh at current heap end
        self.tlab_top = self.values.len();
        self.tlab_limit = self.values.len().saturating_add(self.tlab_size);
        if self.tlab_limit > self.values.len() {
            self.values.resize(self.tlab_limit, None);
            self.ages.resize(self.tlab_limit, 0);
        }
    }

    fn minor_gc_internal(&mut self, roots: &[Reference]) {
        self.minor_gc_count += 1;
        let mut marked = vec![false; self.values.len()];

        // Worklist-based marking from roots.
        let mut worklist: Vec<usize> = roots
            .iter()
            .filter_map(|r| match r {
                Reference::Heap(i) => Some(*i),
                Reference::Null => None,
            })
            .collect();

        // Also trace from remembered set (old->young references)
        for &target in &self.get_remembered_set_references() {
            if target < marked.len() && !marked[target] {
                worklist.push(target);
            }
        }

        while let Some(index) = worklist.pop() {
            if index >= marked.len() || marked[index] {
                continue;
            }
            marked[index] = true;

            // Trace child references.
            if let Some(Some(value)) = self.values.get(index) {
                match value {
                    HeapValue::ReferenceArray { values, .. } => {
                        for r in values {
                            if let Reference::Heap(i) = r {
                                if !marked[*i] {
                                    worklist.push(*i);
                                }
                            }
                        }
                    }
                    HeapValue::Object { fields, .. } => {
                        for v in fields.iter() {
                            if let Value::Reference(Reference::Heap(i)) = v {
                                if !marked[*i] {
                                    worklist.push(*i);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Clear remembered set after minor GC
        self.clear_remembered_set();

        // Sweep young generation: free unmarked, age survivors in place
        let mut freed_count = 0u64;
        let mut freed_bytes = 0u64;
        for i in 0..self.survivor_end {
            if let Some(ref value) = self.values[i] {
                if !marked[i] {
                    freed_bytes += value.heap_size() as u64;
                    freed_count += 1;
                    self.values[i] = None;
                    self.ages[i] = 0;
                } else {
                    self.ages[i] = self.ages[i].saturating_add(1);
                }
            }
        }

        self.live_count = self.values.iter().filter(|v| v.is_some()).count();
        self.allocs_since_gc = 0;

        // After minor GC, start allocating at the current heap end (after survivor space)
        self.tlab_top = self.values.len();
        self.tlab_limit = self.values.len().saturating_add(self.tlab_size);
        self.values.resize(self.tlab_limit, None);
        self.ages.resize(self.tlab_limit, 0);

        self.stats.freed = self.stats.freed.saturating_add(freed_count);
        self.stats.last_collection_freed = freed_count as usize;
        self.stats.freed_bytes = self.stats.freed_bytes.saturating_add(freed_bytes);
    }

    fn major_gc_internal(&mut self, roots: &[Reference]) {
        self.major_gc_count += 1;
        let mut marked = vec![false; self.values.len()];

        // Worklist-based marking from roots.
        let mut worklist: Vec<usize> = roots
            .iter()
            .filter_map(|r| match r {
                Reference::Heap(i) => Some(*i),
                Reference::Null => None,
            })
            .collect();

        while let Some(index) = worklist.pop() {
            if index >= marked.len() || marked[index] {
                continue;
            }
            marked[index] = true;

            if let Some(Some(value)) = self.values.get(index) {
                match value {
                    HeapValue::ReferenceArray { values, .. } => {
                        for r in values {
                            if let Reference::Heap(i) = r {
                                if !marked[*i] {
                                    worklist.push(*i);
                                }
                            }
                        }
                    }
                    HeapValue::Object { fields, .. } => {
                        for v in fields.iter() {
                            if let Value::Reference(Reference::Heap(i)) = v {
                                if !marked[*i] {
                                    worklist.push(*i);
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        // Sweep entire heap.
        let mut freed_count = 0u64;
        let mut freed_bytes = 0u64;
        for i in 0..self.values.len() {
            if let Some(ref value) = self.values[i] {
                if !marked[i] {
                    freed_bytes += value.heap_size() as u64;
                    freed_count += 1;
                    self.values[i] = None;
                    self.ages[i] = 0;
                }
            }
        }

        self.live_count = self.values.iter().filter(|v| v.is_some()).count();
        self.allocs_since_gc = 0;

        // Trim trailing None slots
        while self.values.last().map_or(false, |v| v.is_none()) {
            self.values.pop();
        }
        self.ages.truncate(self.values.len());

        // Reset young generation boundaries after major GC
        self.survivor_end = self.values.len();
        self.young_end = self.values.len();

        self.stats.freed = self.stats.freed.saturating_add(freed_count);
        self.stats.last_collection_freed = freed_count as usize;
        self.stats.freed_bytes = self.stats.freed_bytes.saturating_add(freed_bytes);
    }

    pub(super) fn should_collect(&self) -> bool {
        self.gc_threshold != usize::MAX && self.allocs_since_gc >= self.gc_threshold
    }
}
