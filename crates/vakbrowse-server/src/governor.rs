//! Compute-aware admission control. vakBrowse knows the box it runs on
//! (memory limit incl. cgroups, CPUs) and never launches work blindly:
//!
//! - **Memory budget.** Every browser session and tab holds a lease sized by
//!   a measured per-page cost. A request that does not fit waits in a FIFO
//!   queue; on Linux it must also fit the live free memory (cgroup or host)
//!   above a reserve, so estimates can never push the box into the OOM
//!   killer.
//! - **CPU slots.** Concurrent page work and Chrome launches are capped by
//!   the CPU count; extra requests queue (tokio semaphores are FIFO-fair).
//! - **Nothing is dropped.** A queued request either runs or gets an
//!   explicit, retryable `Busy` answer (queue full, or it waited longer than
//!   the queue timeout). Under pressure the manager hibernates idle sessions
//!   (state kept, browser freed) instead of closing them.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::{Notify, Semaphore};

/// What the host offers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capacity {
    pub memory_mb: u64,
    /// `cgroup` (container limit), `host`, or `assumed`.
    pub memory_source: String,
    pub cpus: usize,
}

/// Admission tunables. Every field has an env override (see `from_env`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernorConfig {
    /// Memory browsers may hold in total (MB).
    pub budget_mb: u64,
    /// Estimated cost of one session with its first page (MB).
    pub session_cost_mb: u64,
    /// Estimated cost of each additional tab (MB).
    pub tab_cost_mb: u64,
    /// Live free memory that must remain after admitting (Linux only).
    pub reserve_mb: u64,
    /// Requests allowed to wait at once; beyond it they get `Busy` at once.
    pub max_queue: usize,
    /// Longest a request waits for capacity before it gets `Busy`.
    pub queue_timeout_secs: u64,
    /// Sessions doing page work at the same time (CPU-bound).
    pub max_active: usize,
    /// Chrome processes starting at the same time (launch is the heaviest step).
    pub max_launches: usize,
    /// Idle seconds before a session is hibernated; None disables.
    pub hibernate_after_secs: Option<u64>,
    /// Under memory pressure, sessions idle at least this long may be
    /// hibernated early to admit a waiting request.
    pub evict_min_idle_secs: u64,
    /// Lean rendering (no images/fonts/autoplay) for sessions that don't say.
    pub lean_default: bool,
}

fn env_u64(name: &str) -> Option<u64> {
    std::env::var(name).ok().and_then(|v| v.trim().parse().ok())
}

impl GovernorConfig {
    /// Defaults derived from the detected capacity, measured on a 2 GB /
    /// 2 vCPU box: ~90 MB per idle session, ~140 MB per heavy page,
    /// ~85 MB per extra tab, ~1.5 CPU-seconds per heavy page load.
    pub fn for_capacity(cap: &Capacity) -> Self {
        Self {
            budget_mb: cap.memory_mb * 6 / 10,
            session_cost_mb: 150,
            tab_cost_mb: 100,
            reserve_mb: (cap.memory_mb / 10).max(256),
            max_queue: 64,
            queue_timeout_secs: 120,
            max_active: (cap.cpus * 2).max(2),
            max_launches: (cap.cpus / 2).max(1),
            hibernate_after_secs: Some(300),
            evict_min_idle_secs: 30,
            lean_default: cap.memory_mb <= 4096,
        }
    }

    pub fn from_env(cap: &Capacity) -> Self {
        let mut c = Self::for_capacity(cap);
        if let Some(v) = env_u64("VAKBROWSE_MEMORY_BUDGET_MB") {
            c.budget_mb = v;
        }
        if let Some(v) = env_u64("VAKBROWSE_SESSION_COST_MB") {
            c.session_cost_mb = v;
        }
        if let Some(v) = env_u64("VAKBROWSE_TAB_COST_MB") {
            c.tab_cost_mb = v;
        }
        if let Some(v) = env_u64("VAKBROWSE_MEMORY_RESERVE_MB") {
            c.reserve_mb = v;
        }
        if let Some(v) = env_u64("VAKBROWSE_MAX_QUEUE") {
            c.max_queue = v as usize;
        }
        if let Some(v) = env_u64("VAKBROWSE_QUEUE_TIMEOUT_SECS") {
            c.queue_timeout_secs = v;
        }
        if let Some(v) = env_u64("VAKBROWSE_MAX_ACTIVE") {
            c.max_active = (v as usize).max(1);
        }
        if let Some(v) = env_u64("VAKBROWSE_MAX_LAUNCHES") {
            c.max_launches = (v as usize).max(1);
        }
        if let Some(v) = env_u64("VAKBROWSE_HIBERNATE_AFTER_SECS") {
            c.hibernate_after_secs = (v > 0).then_some(v);
        }
        if let Some(v) = env_u64("VAKBROWSE_EVICT_IDLE_SECS") {
            c.evict_min_idle_secs = v;
        }
        match std::env::var("VAKBROWSE_LEAN").as_deref() {
            Ok("1" | "true" | "yes") => c.lean_default = true,
            Ok("0" | "false" | "no") => c.lean_default = false,
            _ => {}
        }
        c
    }
}

/// Live view for agents and operators (`status` request).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceStatus {
    pub capacity: Capacity,
    pub budget_mb: u64,
    pub used_mb: u64,
    /// Live free memory (cgroup/host); None where it cannot be read.
    pub available_mb: Option<u64>,
    pub reserve_mb: u64,
    pub session_cost_mb: u64,
    pub tab_cost_mb: u64,
    pub sessions: usize,
    pub hibernated: usize,
    pub queued: usize,
    pub max_queue: usize,
    pub active: usize,
    pub max_active: usize,
    pub launching: usize,
    pub max_launches: usize,
    pub lean_default: bool,
    pub private_network_guard: bool,
}

pub fn detect_capacity() -> Capacity {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1);
    let host = host_memory_mb();
    let (memory_mb, memory_source) = match (cgroup_limit_mb(), host) {
        (Some(c), Some(h)) if c < h => (c, "cgroup"),
        (Some(c), None) => (c, "cgroup"),
        (_, Some(h)) => (h, "host"),
        (None, None) => (4096, "assumed"),
    };
    Capacity {
        memory_mb,
        memory_source: memory_source.into(),
        cpus,
    }
}

#[cfg(target_os = "linux")]
fn read_u64(path: &str) -> Option<u64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

#[cfg(target_os = "linux")]
fn cgroup_dir() -> String {
    // cgroup v2: "0::/path" in /proc/self/cgroup (usually "/" in containers).
    let rel = std::fs::read_to_string("/proc/self/cgroup")
        .ok()
        .and_then(|s| {
            s.lines()
                .find_map(|l| l.strip_prefix("0::").map(|p| p.trim().to_string()))
        })
        .unwrap_or_default();
    let dir = format!("/sys/fs/cgroup{}", rel.trim_end_matches('/'));
    if std::path::Path::new(&format!("{dir}/memory.max")).exists() {
        dir
    } else {
        "/sys/fs/cgroup".into()
    }
}

#[cfg(target_os = "linux")]
fn cgroup_limit_mb() -> Option<u64> {
    let dir = cgroup_dir();
    let v2 = std::fs::read_to_string(format!("{dir}/memory.max")).ok();
    let bytes = match v2.as_deref().map(str::trim) {
        Some("max") => return None,
        Some(v) => v.parse::<u64>().ok()?,
        None => read_u64("/sys/fs/cgroup/memory/memory.limit_in_bytes")?,
    };
    // v1 reports "unlimited" as a huge page-aligned number.
    (bytes < (1u64 << 60)).then_some(bytes >> 20)
}

#[cfg(not(target_os = "linux"))]
fn cgroup_limit_mb() -> Option<u64> {
    None
}

#[cfg(target_os = "linux")]
fn meminfo_mb(key: &str) -> Option<u64> {
    let s = std::fs::read_to_string("/proc/meminfo").ok()?;
    let line = s.lines().find(|l| l.starts_with(key))?;
    let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb >> 10)
}

#[cfg(target_os = "linux")]
fn host_memory_mb() -> Option<u64> {
    meminfo_mb("MemTotal:")
}

#[cfg(target_os = "macos")]
fn host_memory_mb() -> Option<u64> {
    let mut bytes: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    // SAFETY: hw.memsize is a u64; buffer and length match it.
    let rc = unsafe {
        libc::sysctlbyname(
            c"hw.memsize".as_ptr(),
            (&mut bytes as *mut u64).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then_some(bytes >> 20)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn host_memory_mb() -> Option<u64> {
    None
}

/// Live free memory: the tighter of the cgroup headroom and MemAvailable.
#[cfg(target_os = "linux")]
pub fn live_available_mb() -> Option<u64> {
    let host = meminfo_mb("MemAvailable:");
    let dir = cgroup_dir();
    let cg = match (
        cgroup_limit_mb(),
        read_u64(&format!("{dir}/memory.current"))
            .or_else(|| read_u64("/sys/fs/cgroup/memory/memory.usage_in_bytes")),
    ) {
        (Some(limit), Some(cur)) => Some(limit.saturating_sub(cur >> 20)),
        _ => None,
    };
    match (cg, host) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

#[cfg(not(target_os = "linux"))]
pub fn live_available_mb() -> Option<u64> {
    None
}

struct Ledger {
    used_mb: u64,
    queue: VecDeque<u64>,
    next_ticket: u64,
}

pub struct Governor {
    pub cfg: GovernorConfig,
    pub capacity: Capacity,
    ledger: Mutex<Ledger>,
    notify: Notify,
    pub(crate) active: Semaphore,
    pub(crate) launches: Semaphore,
    /// Test hook: overrides the live free-memory probe.
    live_override: Mutex<Option<Option<u64>>>,
}

/// Memory held by a session (and its tabs). Released on drop.
pub struct Lease {
    gov: Arc<Governor>,
    mb: u64,
}

impl Lease {
    pub fn mb(&self) -> u64 {
        self.mb
    }

    /// Fold another lease into this one.
    pub fn absorb(&mut self, mut other: Lease) {
        self.mb += other.mb;
        other.mb = 0;
    }

    /// Give back part of the lease (a tab closed).
    pub fn shrink(&mut self, mb: u64) {
        let mb = mb.min(self.mb);
        self.mb -= mb;
        self.gov.release(mb);
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        if self.mb > 0 {
            self.gov.release(self.mb);
        }
    }
}

/// A place in the admission queue; leaving it (drop) wakes the next waiter.
pub struct Ticket {
    gov: Arc<Governor>,
    id: u64,
}

impl Drop for Ticket {
    fn drop(&mut self) {
        if let Ok(mut l) = self.gov.ledger.lock() {
            l.queue.retain(|t| *t != self.id);
        }
        self.gov.notify.notify_waiters();
    }
}

pub enum Take {
    Granted(Lease),
    /// Someone queued earlier goes first.
    NotYourTurn,
    /// Head of the queue, but the budget or live memory has no room.
    NoRoom,
}

impl Governor {
    pub fn new(capacity: Capacity, cfg: GovernorConfig) -> Arc<Self> {
        Arc::new(Self {
            active: Semaphore::new(cfg.max_active),
            launches: Semaphore::new(cfg.max_launches),
            cfg,
            capacity,
            ledger: Mutex::new(Ledger {
                used_mb: 0,
                queue: VecDeque::new(),
                next_ticket: 0,
            }),
            notify: Notify::new(),
            live_override: Mutex::new(None),
        })
    }

    pub fn from_env() -> Arc<Self> {
        let cap = detect_capacity();
        let cfg = GovernorConfig::from_env(&cap);
        tracing::info!(
            memory_mb = cap.memory_mb,
            source = %cap.memory_source,
            cpus = cap.cpus,
            budget_mb = cfg.budget_mb,
            max_active = cfg.max_active,
            lean = cfg.lean_default,
            "resource governor configured"
        );
        Self::new(cap, cfg)
    }

    #[doc(hidden)]
    pub fn set_live_available_for_test(&self, mb: Option<u64>) {
        if let Ok(mut o) = self.live_override.lock() {
            *o = Some(mb);
        }
    }

    pub fn live_available(&self) -> Option<u64> {
        if let Ok(o) = self.live_override.lock()
            && let Some(v) = *o
        {
            return v;
        }
        live_available_mb()
    }

    pub fn queue_timeout(&self) -> Duration {
        Duration::from_secs(self.cfg.queue_timeout_secs)
    }

    pub fn used_mb(&self) -> u64 {
        self.ledger.lock().map(|l| l.used_mb).unwrap_or(0)
    }

    pub fn queued(&self) -> usize {
        self.ledger.lock().map(|l| l.queue.len()).unwrap_or(0)
    }

    fn release(&self, mb: u64) {
        if let Ok(mut l) = self.ledger.lock() {
            l.used_mb = l.used_mb.saturating_sub(mb);
        }
        self.notify.notify_waiters();
    }

    /// Join the queue, or `Err` when it is already full.
    pub fn enqueue(self: &Arc<Self>) -> std::result::Result<Ticket, String> {
        let mut l = self
            .ledger
            .lock()
            .map_err(|_| "governor poisoned".to_string())?;
        if l.queue.len() >= self.cfg.max_queue {
            return Err(format!(
                "admission queue full ({} waiting); retry later",
                l.queue.len()
            ));
        }
        l.next_ticket += 1;
        let id = l.next_ticket;
        l.queue.push_back(id);
        Ok(Ticket {
            gov: self.clone(),
            id,
        })
    }

    /// Try to take `mb` for the holder of `ticket`.
    pub fn try_take(self: &Arc<Self>, ticket: &Ticket, mb: u64) -> Take {
        let live = self.live_available();
        let Ok(mut l) = self.ledger.lock() else {
            return Take::NoRoom;
        };
        if l.queue.front() != Some(&ticket.id) {
            return Take::NotYourTurn;
        }
        // A single request larger than the whole budget may run alone, so
        // an undersized budget degrades to one-at-a-time, never to deadlock.
        let fits_budget = l.used_mb + mb <= self.cfg.budget_mb || l.used_mb == 0;
        let fits_live = live.is_none_or(|avail| avail >= mb + self.cfg.reserve_mb);
        if !(fits_budget && fits_live) {
            return Take::NoRoom;
        }
        l.used_mb += mb;
        l.queue.pop_front();
        drop(l);
        self.notify.notify_waiters();
        Take::Granted(Lease {
            gov: self.clone(),
            mb,
        })
    }

    /// Charge memory that already exists (a tab the page opened itself):
    /// it cannot be refused, only accounted.
    pub fn force(self: &Arc<Self>, mb: u64) -> Lease {
        if let Ok(mut l) = self.ledger.lock() {
            l.used_mb += mb;
        }
        Lease {
            gov: self.clone(),
            mb,
        }
    }

    /// An empty lease to grow later.
    pub fn empty_lease(self: &Arc<Self>) -> Lease {
        Lease {
            gov: self.clone(),
            mb: 0,
        }
    }

    pub fn notified(&self) -> tokio::sync::futures::Notified<'_> {
        self.notify.notified()
    }

    pub fn status(&self, sessions: usize, hibernated: usize, guard: bool) -> ResourceStatus {
        ResourceStatus {
            capacity: self.capacity.clone(),
            budget_mb: self.cfg.budget_mb,
            used_mb: self.used_mb(),
            available_mb: self.live_available(),
            reserve_mb: self.cfg.reserve_mb,
            session_cost_mb: self.cfg.session_cost_mb,
            tab_cost_mb: self.cfg.tab_cost_mb,
            sessions,
            hibernated,
            queued: self.queued(),
            max_queue: self.cfg.max_queue,
            active: self.cfg.max_active - self.active.available_permits(),
            max_active: self.cfg.max_active,
            launching: self.cfg.max_launches - self.launches.available_permits(),
            max_launches: self.cfg.max_launches,
            lean_default: self.cfg.lean_default,
            private_network_guard: guard,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gov(budget: u64) -> Arc<Governor> {
        let cap = Capacity {
            memory_mb: 2048,
            memory_source: "assumed".into(),
            cpus: 2,
        };
        let mut cfg = GovernorConfig::for_capacity(&cap);
        cfg.budget_mb = budget;
        cfg.max_queue = 2;
        let g = Governor::new(cap, cfg);
        g.set_live_available_for_test(None);
        g
    }

    #[test]
    fn defaults_scale_with_the_box() {
        let small = GovernorConfig::for_capacity(&Capacity {
            memory_mb: 2048,
            memory_source: "cgroup".into(),
            cpus: 2,
        });
        assert_eq!(small.budget_mb, 1228);
        assert_eq!((small.max_active, small.max_launches), (4, 1));
        assert!(small.lean_default);
        let big = GovernorConfig::for_capacity(&Capacity {
            memory_mb: 32768,
            memory_source: "host".into(),
            cpus: 16,
        });
        assert_eq!((big.max_active, big.max_launches), (32, 8));
        assert!(!big.lean_default);
    }

    #[test]
    fn leases_account_and_release() {
        let g = gov(300);
        let t = g.enqueue().unwrap();
        let Take::Granted(mut a) = g.try_take(&t, 150) else {
            panic!()
        };
        assert_eq!(g.used_mb(), 150);
        a.absorb(g.force(100));
        assert_eq!((a.mb(), g.used_mb()), (250, 250));
        a.shrink(100);
        assert_eq!(g.used_mb(), 150);
        drop(a);
        assert_eq!(g.used_mb(), 0);
    }

    #[test]
    fn fifo_and_no_room() {
        let g = gov(200);
        let t1 = g.enqueue().unwrap();
        let t2 = g.enqueue().unwrap();
        assert!(matches!(g.try_take(&t2, 50), Take::NotYourTurn));
        let Take::Granted(l1) = g.try_take(&t1, 150) else {
            panic!()
        };
        assert!(matches!(g.try_take(&t2, 150), Take::NoRoom));
        drop(l1);
        assert!(matches!(g.try_take(&t2, 150), Take::Granted(_)));
    }

    #[test]
    fn queue_is_bounded() {
        let g = gov(200);
        let _a = g.enqueue().unwrap();
        let _b = g.enqueue().unwrap();
        assert!(g.enqueue().is_err());
    }

    #[test]
    fn oversized_request_runs_alone_and_live_memory_gates() {
        let g = gov(100);
        let t = g.enqueue().unwrap();
        let Take::Granted(l) = g.try_take(&t, 500) else {
            panic!()
        };
        drop(l);
        g.set_live_available_for_test(Some(300));
        let t = g.enqueue().unwrap();
        // 50 + reserve(256) > 300 free → must wait despite budget room.
        assert!(matches!(g.try_take(&t, 50), Take::NoRoom));
        g.set_live_available_for_test(Some(1000));
        assert!(matches!(g.try_take(&t, 50), Take::Granted(_)));
    }

    #[test]
    fn detect_capacity_is_sane() {
        let c = detect_capacity();
        assert!(c.memory_mb > 0 && c.cpus > 0);
    }
}
