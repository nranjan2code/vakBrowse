//! Shared helpers for engine integration tests.
//!
//! Browser launch is heavy and contends for OS resources (memory, GPU,
//! user-namespace slots) — especially as non-root on a constrained container,
//! where four parallel chrome launches can all hit the sandbox and thrash.
//! These tests are therefore **serialized within each test binary** via a
//! shared semaphore so `cargo test` (default parallel) stays green everywhere.

use std::sync::OnceLock;

use tokio::sync::Semaphore;

pub fn browser_lock() -> &'static Semaphore {
    static LOCK: OnceLock<Semaphore> = OnceLock::new();
    LOCK.get_or_init(|| Semaphore::new(1))
}
