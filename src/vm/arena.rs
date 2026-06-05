/// Lightweight string arena for class metadata deduplication.
///
/// Acts as a simple bump-style intern table for string names used in class
/// metadata (class names, descriptors, field names). Reduces per-string
/// Box allocations for repeated names across many loaded classes.
///
/// Full arena migration (replacing per-class HashMaps with slab allocations)
/// is a larger refactor deferred until RSS profiling shows it worthwhile.
pub struct ClassMetaArena {
    strings: Vec<Box<str>>,
}

impl ClassMetaArena {
    pub fn new() -> Self {
        Self { strings: Vec::new() }
    }

    /// Intern a string, returning a reference into the arena's storage.
    /// Linear scan — suitable for metadata strings (small sets, rare churn).
    pub fn intern(&mut self, s: &str) -> &str {
        if let Some(pos) = self.strings.iter().position(|e| e.as_ref() == s) {
            return self.strings[pos].as_ref();
        }
        self.strings.push(s.into());
        self.strings.last().unwrap().as_ref()
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}

impl Default for ClassMetaArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_interns_and_deduplicates() {
        let mut arena = ClassMetaArena::new();
        let a = arena.intern("java/lang/Object") as *const str;
        let b = arena.intern("java/lang/Object") as *const str;
        assert_eq!(a, b, "same string should return same pointer");
        assert_eq!(arena.len(), 1);
    }

    #[test]
    fn arena_stores_distinct_strings() {
        let mut arena = ClassMetaArena::new();
        arena.intern("java/lang/String");
        arena.intern("java/lang/Integer");
        assert_eq!(arena.len(), 2);
    }
}
