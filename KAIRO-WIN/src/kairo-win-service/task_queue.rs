use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::env;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use once_cell::sync::Lazy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub command: String,
}

#[derive(Default)]
pub struct TaskQueue {
    tasks: Vec<Task>,
}

impl TaskQueue {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    pub fn add_task(&mut self, task: Task) {
        self.tasks.push(task);
    }
}

const DEFAULT_QUEUE_WINDOW_MS: u64 = 1_000;
const DEFAULT_QUEUE_BURST_LIMIT: usize = 24;
const DEFAULT_QUEUE_PENDING_LIMIT: usize = 8;
const DEFAULT_QUEUE_SUBJECT_TTL_MS: u64 = 60_000;

static GLOBAL_QUEUE_GUARD: Lazy<Mutex<Arc<QueuePressureGuard>>> =
    Lazy::new(|| Mutex::new(Arc::new(QueuePressureGuard::default())));

#[derive(Debug, Clone)]
pub struct QueuePressureConfig {
    pub window_ms: u64,
    pub burst_limit: usize,
    pub pending_limit: usize,
    pub subject_ttl_ms: u64,
}

impl Default for QueuePressureConfig {
    fn default() -> Self {
        Self {
            window_ms: DEFAULT_QUEUE_WINDOW_MS,
            burst_limit: DEFAULT_QUEUE_BURST_LIMIT,
            pending_limit: DEFAULT_QUEUE_PENDING_LIMIT,
            subject_ttl_ms: DEFAULT_QUEUE_SUBJECT_TTL_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueueDropReason {
    BurstRateExceeded,
    PendingDepthExceeded,
}

impl QueueDropReason {
    pub fn code(&self) -> u16 {
        match self {
            Self::BurstRateExceeded => 0x1201,
            Self::PendingDepthExceeded => 0x1202,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::BurstRateExceeded => "queue_burst_drop",
            Self::PendingDepthExceeded => "queue_pending_drop",
        }
    }
}

#[derive(Debug)]
pub struct QueuePressureGuard {
    config: QueuePressureConfig,
    subjects: Mutex<HashMap<String, QueueSubjectState>>,
}

#[derive(Debug, Default)]
struct QueueSubjectState {
    observed: VecDeque<u64>,
    pending: usize,
    last_seen_ms: u64,
}

#[derive(Debug)]
pub struct QueueLease {
    guard: Arc<QueuePressureGuard>,
    subject_key: String,
    active: bool,
}

impl Default for QueuePressureGuard {
    fn default() -> Self {
        Self::new(QueuePressureConfig::default())
    }
}

impl QueuePressureGuard {
    pub fn new(config: QueuePressureConfig) -> Self {
        Self {
            config,
            subjects: Mutex::new(HashMap::new()),
        }
    }

    pub fn begin(self: &Arc<Self>, subject_key: &str) -> Result<QueueLease, QueueDropReason> {
        let now_ms = unix_ms_now();
        let mut guard = self.subjects.lock().unwrap();
        self.gc_locked(&mut guard, now_ms);
        let subject = guard.entry(subject_key.to_string()).or_default();
        subject.last_seen_ms = now_ms;

        self.prune_observed(subject, now_ms);

        if subject.observed.len() >= self.config.burst_limit {
            return Err(QueueDropReason::BurstRateExceeded);
        }
        if subject.pending >= self.config.pending_limit {
            return Err(QueueDropReason::PendingDepthExceeded);
        }

        subject.observed.push_back(now_ms);
        subject.pending = subject.pending.saturating_add(1);
        drop(guard);

        Ok(QueueLease {
            guard: self.clone(),
            subject_key: subject_key.to_string(),
            active: true,
        })
    }

    pub fn is_congested(&self) -> bool {
        let guard = self.subjects.lock().unwrap();
        let total_pending: usize = guard.values().map(|s| s.pending).sum();
        // Trigger congestion mode if total pending tasks exceed a global threshold.
        // For simplicity, we use a multiple of the per-subject limit.
        total_pending > (self.config.pending_limit * 2)
    }
}

impl Drop for QueueLease {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        let mut guard = self.guard.subjects.lock().unwrap();
        if let Some(subject) = guard.get_mut(self.subject_key.as_str()) {
            subject.pending = subject.pending.saturating_sub(1);
            subject.last_seen_ms = unix_ms_now();
            self.guard.prune_observed(subject, subject.last_seen_ms);
        }
        self.guard.gc_locked(&mut guard, unix_ms_now());
        self.active = false;
    }
}

pub fn global_queue_pressure_guard() -> Arc<QueuePressureGuard> {
    GLOBAL_QUEUE_GUARD.lock().unwrap().clone()
}

pub fn set_global_queue_pressure_guard(guard: Arc<QueuePressureGuard>) {
    *GLOBAL_QUEUE_GUARD.lock().unwrap() = guard;
}

pub fn configure_global_queue_pressure_guard_from_env() -> Result<Arc<QueuePressureGuard>, String> {
    let config = QueuePressureConfig {
        window_ms: env_nonempty("KAIRO_QUEUE_WINDOW_MS")
            .map(|raw| {
                raw.parse::<u64>()
                    .map_err(|e| format!("invalid KAIRO_QUEUE_WINDOW_MS: {}", e))
            })
            .transpose()?
            .unwrap_or(DEFAULT_QUEUE_WINDOW_MS),
        burst_limit: env_nonempty("KAIRO_QUEUE_BURST_LIMIT")
            .map(|raw| {
                raw.parse::<usize>()
                    .map_err(|e| format!("invalid KAIRO_QUEUE_BURST_LIMIT: {}", e))
            })
            .transpose()?
            .unwrap_or(DEFAULT_QUEUE_BURST_LIMIT),
        pending_limit: env_nonempty("KAIRO_QUEUE_PENDING_LIMIT")
            .map(|raw| {
                raw.parse::<usize>()
                    .map_err(|e| format!("invalid KAIRO_QUEUE_PENDING_LIMIT: {}", e))
            })
            .transpose()?
            .unwrap_or(DEFAULT_QUEUE_PENDING_LIMIT),
        subject_ttl_ms: env_nonempty("KAIRO_QUEUE_SUBJECT_TTL_MS")
            .map(|raw| {
                raw.parse::<u64>()
                    .map_err(|e| format!("invalid KAIRO_QUEUE_SUBJECT_TTL_MS: {}", e))
            })
            .transpose()?
            .unwrap_or(DEFAULT_QUEUE_SUBJECT_TTL_MS),
    };
    let guard = Arc::new(QueuePressureGuard::new(config));
    set_global_queue_pressure_guard(guard.clone());
    Ok(guard)
}

fn env_nonempty(name: &str) -> Option<String> {
    env::var(name).ok().and_then(|raw| {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn unix_ms_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

impl QueuePressureGuard {
    fn prune_observed(&self, subject: &mut QueueSubjectState, now_ms: u64) {
        while let Some(front) = subject.observed.front().copied() {
            if now_ms.saturating_sub(front) <= self.config.window_ms {
                break;
            }
            subject.observed.pop_front();
        }
    }

    fn gc_locked(&self, subjects: &mut HashMap<String, QueueSubjectState>, now_ms: u64) {
        let ttl_ms = self.config.subject_ttl_ms.max(self.config.window_ms);
        subjects.retain(|_, subject| {
            self.prune_observed(subject, now_ms);
            if subject.pending > 0 || !subject.observed.is_empty() {
                return true;
            }
            now_ms.saturating_sub(subject.last_seen_ms) <= ttl_ms
        });
    }

    #[cfg(test)]
    fn subject_count(&self) -> usize {
        self.subjects.lock().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn burst_growth_is_dropped() {
        let guard = Arc::new(QueuePressureGuard::new(QueuePressureConfig {
            window_ms: 5_000,
            burst_limit: 2,
            pending_limit: 8,
            subject_ttl_ms: 5_000,
        }));

        let _first = guard.begin("agent-alpha::session-01").unwrap();
        let _second = guard.begin("agent-alpha::session-01").unwrap();

        assert_eq!(
            guard.begin("agent-alpha::session-01").unwrap_err(),
            QueueDropReason::BurstRateExceeded
        );
    }

    #[test]
    fn pending_depth_is_dropped_until_release() {
        let guard = Arc::new(QueuePressureGuard::new(QueuePressureConfig {
            window_ms: 5_000,
            burst_limit: 8,
            pending_limit: 1,
            subject_ttl_ms: 5_000,
        }));

        let first = guard.begin("agent-alpha::session-01").unwrap();
        assert_eq!(
            guard.begin("agent-alpha::session-01").unwrap_err(),
            QueueDropReason::PendingDepthExceeded
        );
        drop(first);
        assert!(guard.begin("agent-alpha::session-01").is_ok());
    }

    #[test]
    fn empty_subjects_are_reclaimed_after_ttl() {
        let guard = Arc::new(QueuePressureGuard::new(QueuePressureConfig {
            window_ms: 1,
            burst_limit: 8,
            pending_limit: 1,
            subject_ttl_ms: 2,
        }));

        let lease = guard.begin("agent-alpha::session-01").unwrap();
        drop(lease);
        assert_eq!(guard.subject_count(), 1);

        sleep(Duration::from_millis(8));
        let _ = guard.begin("agent-beta::session-02").unwrap();

        assert_eq!(guard.subject_count(), 1);
    }
}
