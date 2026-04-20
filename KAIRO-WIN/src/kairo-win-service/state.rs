use arc_swap::ArcSwap;
use once_cell::sync::OnceCell;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::matcher::RuleSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownMode {
    DropAndForceOff,
    DropOnly,
}

pub struct KairoState {
    enabled: AtomicBool,
    rules: ArcSwap<RuleSet>,
    unknown_mode: ArcSwap<UnknownMode>,
    acl_valid: AtomicBool,
    allow_count: AtomicUsize,

    // AI Server List Password Management
    ai_pass_hash: parking_lot::RwLock<Option<String>>,
    ai_fail_count: AtomicUsize,
    ai_locked_until: AtomicU64,
    ai_enabled: AtomicBool,
}
static GLOBAL_STATE: OnceCell<Arc<KairoState>> = OnceCell::new();

impl KairoState {
    pub fn new(initial_rules: RuleSet) -> Self {
        Self {
            enabled: AtomicBool::new(false),
            rules: ArcSwap::from_pointee(initial_rules),
            unknown_mode: ArcSwap::from_pointee(UnknownMode::DropAndForceOff),
            acl_valid: AtomicBool::new(true),
            allow_count: AtomicUsize::new(0),
            ai_pass_hash: parking_lot::RwLock::new(None),
            ai_fail_count: AtomicUsize::new(0),
            ai_locked_until: AtomicU64::new(0),
            ai_enabled: AtomicBool::new(false),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    pub fn set_enabled(&self, on: bool) {
        self.enabled.store(on, Ordering::Relaxed);
    }

    pub fn swap_rules(&self, rules: RuleSet) {
        self.rules.store(Arc::new(rules));
    }

    pub fn rules(&self) -> Arc<RuleSet> {
        self.rules.load_full()
    }

    pub fn unknown_mode(&self) -> UnknownMode {
        *self.unknown_mode.load_full()
    }

    pub fn set_unknown_mode(&self, mode: UnknownMode) {
        self.unknown_mode.store(Arc::new(mode));
    }

    pub fn mark_acl_status(&self, valid: bool, allow_count: usize) {
        self.acl_valid.store(valid, Ordering::Relaxed);
        self.allow_count.store(allow_count, Ordering::Relaxed);
    }

    pub fn can_switch_on(&self) -> Result<(), &'static str> {
        if !self.acl_valid.load(Ordering::Relaxed) {
            return Err("acl_invalid");
        }
        if self.allow_count.load(Ordering::Relaxed) == 0 {
            return Err("allow_empty");
        }
        Ok(())
    }

    pub fn set_ai_password(&self, pass: &str) {
        let hash = format!("{:x}", md5::compute(pass));
        *self.ai_pass_hash.write() = Some(hash);
    }

    pub fn check_ai_password(&self, pass: &str) -> Result<bool, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let locked_until = self.ai_locked_until.load(Ordering::SeqCst);

        if now < locked_until {
            let wait = locked_until - now;
            return Err(format!("Locked. Please wait {} seconds.", wait));
        }

        let hash_guard = self.ai_pass_hash.read();
        let Some(ref stored_hash) = *hash_guard else {
            return Err("Password not set. Please set a password first.".to_string());
        };

        let input_hash = format!("{:x}", md5::compute(pass));
        if input_hash == *stored_hash {
            self.ai_fail_count.store(0, Ordering::SeqCst);
            self.ai_enabled.store(true, Ordering::SeqCst);
            Ok(true)
        } else {
            let fails = self.ai_fail_count.fetch_add(1, Ordering::SeqCst) + 1;
            if fails >= 3 {
                self.ai_locked_until.store(now + 300, Ordering::SeqCst);
                Err("Too many failures. Locked for 5 minutes.".to_string())
            } else {
                Err(format!(
                    "Incorrect password. {} attempts remaining.",
                    3 - fails
                ))
            }
        }
    }

    pub fn set_ai_enabled(&self, enabled: bool) {
        self.ai_enabled.store(enabled, Ordering::SeqCst);
    }

    pub fn is_ai_enabled(&self) -> bool {
        self.ai_enabled.load(Ordering::SeqCst)
    }

    pub fn has_ai_password(&self) -> bool {
        self.ai_pass_hash.read().is_some()
    }
}

pub fn set_global_state(state: Arc<KairoState>) {
    let _ = GLOBAL_STATE.set(state);
}

pub fn global_state() -> Option<Arc<KairoState>> {
    GLOBAL_STATE.get().cloned()
}
