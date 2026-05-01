//! Heap storage: `HeapValue` variants, `Heap` with mark-and-sweep GC, and
//! `GcStats` counters. Accessed through `Vm::heap` (behind a `Mutex`).

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
        fields: HashMap<String, Value>,
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
}

impl Default for Heap {
    fn default() -> Self {
        Self {
            values: Vec::new(),
            live_count: 0,
            allocs_since_gc: 0,
            gc_threshold: 1024,
            stats: GcStats::default(),
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
        // Try to reuse a freed slot.
        for (i, slot) in self.values.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(value);
                return Reference::Heap(i);
            }
        }
        let reference = self.values.len();
        self.values.push(Some(value));
        Reference::Heap(reference)
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
        let values = match self.get_mut(reference)? {
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
    pub(super) fn gc(&mut self, roots: &[Reference]) {
        let mut marked = vec![false; self.values.len()];

        // Worklist-based marking.
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
                        for v in fields.values() {
                            if let Value::Reference(Reference::Heap(i)) = v {
                                if !marked[*i] {
                                    worklist.push(*i);
                                }
                            }
                        }
                    }
                    HeapValue::IntArray { .. }
                    | HeapValue::LongArray { .. }
                    | HeapValue::FloatArray { .. }
                    | HeapValue::DoubleArray { .. }
                    | HeapValue::String(_)
                    | HeapValue::StringBuilder(_) => {}
                }
            }
        }

        // Sweep: free unmarked objects.
        let mut freed = 0u64;
        for (i, slot) in self.values.iter_mut().enumerate() {
            if slot.is_some() && !marked[i] {
                *slot = None;
                freed += 1;
            }
        }
        self.live_count = self.values.iter().filter(|v| v.is_some()).count();
        self.allocs_since_gc = 0;

        // Trim trailing None slots.
        while self.values.last().map_or(false, |v| v.is_none()) {
            self.values.pop();
        }

        self.stats.collections = self.stats.collections.saturating_add(1);
        self.stats.freed = self.stats.freed.saturating_add(freed);
        self.stats.live = self.live_count;
    }

    pub(super) fn should_collect(&self) -> bool {
        self.gc_threshold != usize::MAX && self.allocs_since_gc >= self.gc_threshold
    }
}
