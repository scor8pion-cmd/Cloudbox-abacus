//! Watches local folders of "realtime" sync profiles and triggers a sync
//! after a 2-second quiet period (debounce).

use crate::config::SyncProfile;
use crate::database::Database;
use crate::sync_engine::is_excluded;
use anyhow::Result;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const DEBOUNCE: Duration = Duration::from_secs(2);

pub struct FileWatcher {
    // Dropping the watcher stops the background thread (channel closes).
    _watcher: RecommendedWatcher,
}

impl FileWatcher {
    /// `on_change(profile_id)` is invoked once per debounced batch.
    pub fn start<F>(profiles: Vec<SyncProfile>, db: Arc<Mutex<Database>>, on_change: F) -> Result<Option<Self>>
    where
        F: Fn(String) + Send + 'static,
    {
        let profiles: Vec<SyncProfile> = profiles
            .into_iter()
            .filter(|p| p.enabled && p.schedule == "realtime" && Path::new(&p.local_path).is_dir())
            .collect();
        if profiles.is_empty() {
            return Ok(None);
        }

        let (tx, rx) = channel::<notify::Result<Event>>();
        let mut watcher = notify::recommended_watcher(tx)?;
        for p in &profiles {
            watcher.watch(Path::new(&p.local_path), RecursiveMode::Recursive)?;
        }

        std::thread::spawn(move || {
            // profile_id -> (last event time, changed paths)
            let mut pending: HashMap<String, (Instant, Vec<PathBuf>)> = HashMap::new();
            loop {
                match rx.recv_timeout(Duration::from_millis(500)) {
                    Ok(Ok(event)) => {
                        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_) | EventKind::Remove(_)) {
                            continue;
                        }
                        for path in event.paths {
                            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                            if let Some(p) = profiles.iter().find(|p| path.starts_with(&p.local_path)) {
                                if is_excluded(&name, &p.exclude) {
                                    continue;
                                }
                                let entry = pending.entry(p.id.clone()).or_insert_with(|| (Instant::now(), Vec::new()));
                                entry.0 = Instant::now();
                                entry.1.push(path);
                            }
                        }
                    }
                    Ok(Err(_)) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => break,
                }

                let ready: Vec<String> = pending.iter().filter(|(_, (t, _))| t.elapsed() >= DEBOUNCE).map(|(id, _)| id.clone()).collect();
                for id in ready {
                    if let Some((_, paths)) = pending.remove(&id) {
                        if let Ok(db) = db.lock() {
                            for path in paths.iter().filter(|p| p.is_file()) {
                                db.enqueue_unique("upload", &path.to_string_lossy(), "").ok();
                            }
                        }
                        on_change(id);
                    }
                }
            }
        });

        Ok(Some(Self { _watcher: watcher }))
    }
}
