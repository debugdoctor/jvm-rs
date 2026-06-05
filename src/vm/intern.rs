use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// A shared string interner that deduplicates strings across the VM.
/// Class names, method names, and descriptor strings that are interned
/// will share the same `Arc<str>` allocation, reducing clone overhead.
pub struct Interner {
    inner: Mutex<HashMap<String, Arc<str>>>,
}

impl Interner {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// Intern a `&str`, returning a shared `Arc<str>`. If the string was
    /// already interned, the existing `Arc` is returned (same pointer).
    pub fn intern(&self, s: &str) -> Arc<str> {
        let mut map = self.inner.lock().unwrap();
        if let Some(existing) = map.get(s) {
            return Arc::clone(existing);
        }
        let arc: Arc<str> = Arc::from(s);
        map.insert(s.to_string(), Arc::clone(&arc));
        arc
    }

    /// Intern an owned `String`.
    pub fn intern_string(&self, s: String) -> Arc<str> {
        let mut map = self.inner.lock().unwrap();
        if let Some(existing) = map.get(s.as_str()) {
            return Arc::clone(existing);
        }
        let arc: Arc<str> = Arc::from(s.as_str());
        map.insert(s, Arc::clone(&arc));
        arc
    }
}

static INTERNER: OnceLock<Interner> = OnceLock::new();

/// Returns the global VM string interner.
pub fn get_interner() -> &'static Interner {
    INTERNER.get_or_init(Interner::new)
}
