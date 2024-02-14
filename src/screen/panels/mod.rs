mod colors;
mod console;
mod header;
mod preview;
mod sections;
mod patterns;
mod project;

use std::{
    collections::{hash_set, HashSet},
    hash::{DefaultHasher, Hash, Hasher},
    marker::PhantomData,
    time::{self, Duration, Instant},
};

pub use colors::ColorsPanel;
pub use console::ConsolePanel;
pub use header::HeaderPanel;
pub use preview::PreviewPanel;
pub use sections::SectionsPanel;
pub use patterns::PatternsPanel;
pub use project::ProjectPanel;

struct FieldFlags<T> {
    flagged: HashSet<T>,
}

impl<T: Eq + Hash> FieldFlags<T> {
    fn new() -> Self {
        FieldFlags {
            flagged: HashSet::new(),
        }
    }

    fn flag(&mut self, f: T) { self.flagged.insert(f); }

    fn drain(&mut self) -> hash_set::Drain<'_, T> { self.flagged.drain() }
}

struct StateMonitor<T> {
    current_hash: u64,
    last_modification_time: Instant,
    type_ref: PhantomData<T>,
}

impl<T: Hash> StateMonitor<T> {
    fn new() -> Self {
        StateMonitor {
            current_hash: 0,
            last_modification_time: Instant::now(),
            type_ref: Default::default(),
        }
    }

    fn advance(&mut self, time: time::Duration) {
        self.last_modification_time = Instant::now() + time;
    }

    fn time_elapsed(&mut self, time: time::Duration) -> bool {
        self.last_modification_time.elapsed() >= time
    }

    fn sleep(&mut self) { self.advance(Duration::from_secs(31536000)) }

    fn hash_code(obj: &T) -> u64 {
        let mut hasher = DefaultHasher::new();
        obj.hash(&mut hasher);
        hasher.finish()
    }

    fn update(&mut self, obj: &T) {
        let code = StateMonitor::hash_code(obj);
        if code != self.current_hash {
            self.current_hash = code;
            self.last_modification_time = Instant::now();
        }
    }
}
