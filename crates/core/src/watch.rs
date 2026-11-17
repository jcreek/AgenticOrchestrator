use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

struct Watch {
    revision: Arc<AtomicU32>,
    _watcher: RecommendedWatcher,
}

#[derive(Default)]
struct Live {
    watches: HashMap<String, Watch>,
    suspended: HashSet<String>,
}

/// Counts changes inside workspace folders, so callers run git only when something changed.
/// A watch keeps its folder open, so suspend or forget it before deleting the folder.
#[derive(Default)]
pub struct Watches {
    live: Mutex<Live>,
}

impl Watches {
    /// Starts watching `folder` unless it is already watched or suspended. A folder that
    /// cannot be watched is left alone and has no revision; callers then poll on a timer.
    pub fn ensure(&self, id: &str, folder: &Path) {
        let mut live = lock(&self.live);
        if live.watches.contains_key(id) || live.suspended.contains(id) {
            return;
        }
        let revision = Arc::new(AtomicU32::new(0));
        let counter = Arc::clone(&revision);
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            if event.is_ok() {
                counter.fetch_add(1, Ordering::Relaxed);
            }
        });
        if let Ok(mut watcher) = watcher
            && watcher.watch(folder, RecursiveMode::Recursive).is_ok()
        {
            live.watches.insert(id.to_owned(), Watch { revision, _watcher: watcher });
        }
    }

    /// Stops watching and refuses to start again until `resume`.
    pub fn suspend(&self, id: &str) {
        let mut live = lock(&self.live);
        live.watches.remove(id);
        live.suspended.insert(id.to_owned());
    }

    pub fn resume(&self, id: &str) {
        lock(&self.live).suspended.remove(id);
    }

    pub fn forget(&self, id: &str) {
        let mut live = lock(&self.live);
        live.watches.remove(id);
        live.suspended.remove(id);
    }

    /// A number per watched workspace that differs from an earlier reading if its folder changed in between.
    pub fn revisions(&self) -> HashMap<String, u32> {
        let live = lock(&self.live);
        live.watches.iter().map(|(id, watch)| (id.clone(), watch.revision.load(Ordering::Relaxed))).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::thread::sleep;
    use std::time::{Duration, Instant};

    fn revision_after(watches: &Watches, earlier: Option<u32>, timeout: Duration) -> Option<u32> {
        let deadline = Instant::now() + timeout;
        loop {
            let now = watches.revisions().get("w").copied();
            if now != earlier || Instant::now() > deadline {
                return now;
            }
            sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn a_change_deep_inside_the_folder_moves_the_revision_and_quiet_leaves_it() {
        let folder = tempfile::tempdir().expect("tempdir");
        fs::create_dir_all(folder.path().join("a/b")).expect("dirs");
        let watches = Watches::default();
        watches.ensure("w", folder.path());
        assert_eq!(watches.revisions().get("w"), Some(&0));
        fs::write(folder.path().join("a/b/file.txt"), "x").expect("write");
        let changed = revision_after(&watches, Some(0), Duration::from_secs(5));
        assert_ne!(changed, Some(0));
        sleep(Duration::from_millis(300));
        let settled = watches.revisions().get("w").copied();
        assert_eq!(revision_after(&watches, settled, Duration::from_millis(300)), settled);
    }

    #[test]
    fn a_suspended_folder_can_be_deleted_and_is_not_watched_again_until_resumed() {
        let folder = tempfile::tempdir().expect("tempdir");
        let root = folder.path().join("worktree");
        fs::create_dir_all(root.join("src")).expect("dirs");
        let watches = Watches::default();
        watches.ensure("w", &root);
        watches.suspend("w");
        watches.ensure("w", &root);
        assert_eq!(watches.revisions().get("w"), None);
        fs::remove_dir_all(&root).expect("delete");
        assert!(!root.exists());

        fs::create_dir_all(&root).expect("recreate");
        watches.resume("w");
        watches.ensure("w", &root);
        fs::write(root.join("file.txt"), "x").expect("write");
        assert_ne!(revision_after(&watches, Some(0), Duration::from_secs(5)), Some(0));
    }

    #[test]
    fn a_missing_folder_has_no_revision_and_does_not_fail() {
        let watches = Watches::default();
        watches.ensure("w", Path::new(r"C:\definitely\not\here"));
        assert!(watches.revisions().is_empty());
    }
}
