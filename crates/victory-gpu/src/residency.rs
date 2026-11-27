//! Host-memory admission for speculative texture work and reusable upload storage.
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;
static PRESSURED: AtomicBool = AtomicBool::new(false);
static SHARED_CRITICAL: AtomicBool = AtomicBool::new(false);
static HEADROOM: AtomicU64 = AtomicU64::new(u64::MAX);

fn memory_status() -> Option<MEMORYSTATUSEX> {
    // SAFETY: the API initializes the fields of a correctly sized structure.
    let mut status: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
    status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
    (unsafe { GlobalMemoryStatusEx(&mut status) } != 0).then_some(status)
}

// The texture sweep may run for seconds inside a single submitted frame.
static NEXT_COMMIT_SAMPLE: AtomicU64 = AtomicU64::new(0);
static AVAILABLE_COMMIT: AtomicU64 = AtomicU64::new(u64::MAX);
static RETRY_UPLOAD_AT: AtomicU64 = AtomicU64::new(0);

pub(crate) fn allocation_error(error: &windows::core::Error) {
    if error.code() == windows::Win32::Foundation::E_OUTOFMEMORY {
        RETRY_UPLOAD_AT.fetch_max(crate::host::coarse_ms().saturating_add(500), Ordering::Relaxed);
    }
}

thread_local! {
    static STAGING_DEFERRED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub(crate) fn note_staging_deferred() {
    STAGING_DEFERRED.set(true);
}

pub(crate) fn take_staging_deferred() -> bool {
    STAGING_DEFERRED.replace(false)
}

pub(crate) fn upload_refused() -> bool {
    crate::host::coarse_ms() < RETRY_UPLOAD_AT.load(Ordering::Relaxed)
}

fn sample_commit(now: u64) -> u64 {
    let next = NEXT_COMMIT_SAMPLE.load(Ordering::Relaxed);
    if now >= next
        && NEXT_COMMIT_SAMPLE
            .compare_exchange(next, now.saturating_add(32), Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        && let Some(status) = memory_status()
    {
        AVAILABLE_COMMIT.store(status.ullAvailPageFile, Ordering::Relaxed);
    }
    AVAILABLE_COMMIT.load(Ordering::Relaxed)
}

fn has_upload_reserve(commit: u64, bake: bool) -> bool {
    // Keep a protective floor against Rust/game OOM aborts without starving
    // bindless world textures or one-shot ped decoration bakes when steady-state
    // commit headroom sits in the 1-4 GiB range on 32 GiB hosts.
    commit >= if bake { 256 * MIB } else { 512 * MIB }
}

fn has_growth_reserve(commit: u64, bake: bool) -> bool {
    // Growing a fresh 64 MiB upload block for speculative bindless prefetch
    // requires more headroom than reusing slices already committed inside the
    // staging pool.
    commit >= if bake { 256 * MIB } else { 1024 * MIB }
}

/// Leave commit for game allocations and submissions already in flight while
/// still letting bindless textures and one-shot material bakes upload.
pub(crate) fn admit_texture_pass(bake: bool) -> bool {
    let now = crate::host::coarse_ms();
    if now < RETRY_UPLOAD_AT.load(Ordering::Relaxed) {
        return false;
    }
    has_upload_reserve(sample_commit(now), bake)
}

/// Whether the staging allocator may commit a brand-new OS upload buffer
/// (reusing existing free ranges in already-committed pool blocks stays open
/// whenever `admit_texture_pass` is true).
pub(crate) fn allow_staging_growth(bake: bool) -> bool {
    let now = crate::host::coarse_ms();
    if now < RETRY_UPLOAD_AT.load(Ordering::Relaxed) {
        return false;
    }
    has_growth_reserve(sample_commit(now), bake)
}

pub(crate) fn limited_host_ram() -> bool {
    static LIMITED: OnceLock<bool> = OnceLock::new();
    *LIMITED.get_or_init(|| memory_status().is_some_and(|s| s.ullTotalPhys <= 32 * GIB))
}

/// True on ~16 GB (and smaller) laptops where aggressive upload/staging caps
/// prevent private commit and swap thrashing (`<= 20 GiB` physical RAM).
pub(crate) fn very_low_ram() -> bool {
    static VERY_LOW: OnceLock<bool> = OnceLock::new();
    *VERY_LOW.get_or_init(|| memory_status().is_some_and(|s| s.ullTotalPhys <= 20 * GIB))
}

fn next_pressure_for(was: bool, physical: u64, commit: u64, very_low: bool) -> bool {
    if very_low {
        // On ~16 GB hosts with a large pagefile, Windows routinely keeps
        // Standby lists full (200-1500 MiB `ullAvailPhys`) while commit
        // headroom is ample; avoid throttling unless commit is low, both are
        // tight, or free physical RAM drops to critically low levels.
        if was {
            commit < 4 * GIB || (physical < 768 * MIB && commit < 6 * GIB) || physical < 192 * MIB
        } else {
            commit <= 3 * GIB || (physical <= 384 * MIB && commit <= 5 * GIB) || physical <= 128 * MIB
        }
    } else if was {
        physical < 5 * GIB || commit < 6 * GIB
    } else {
        physical <= 3 * GIB || commit <= 4 * GIB
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn next_pressure(was: bool, physical: u64, commit: u64) -> bool {
    next_pressure_for(was, physical, commit, false)
}

fn next_pressure_with_shared_for(
    was: bool,
    physical: u64,
    commit: u64,
    shared_budget: u64,
    shared_used: u64,
    very_low: bool,
) -> bool {
    if next_pressure_for(was, physical, commit, very_low) {
        return true;
    }
    if shared_budget == 0 {
        return false;
    }
    let (enter_pct, exit_pct) = if very_low { (86, 78) } else { (72, 64) };
    if was {
        u128::from(shared_used) * 100 >= u128::from(shared_budget) * exit_pct
    } else {
        u128::from(shared_used) * 100 >= u128::from(shared_budget) * enter_pct
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn next_pressure_with_shared(
    was: bool,
    physical: u64,
    commit: u64,
    shared_budget: u64,
    shared_used: u64,
) -> bool {
    next_pressure_with_shared_for(was, physical, commit, shared_budget, shared_used, false)
}

fn shared_is_critical_for(shared_budget: u64, shared_used: u64, very_low: bool) -> bool {
    let pct = if very_low { 92 } else { 85 };
    shared_budget > 0 && u128::from(shared_used) * 100 >= u128::from(shared_budget) * pct
}

#[cfg_attr(not(test), allow(dead_code))]
fn shared_is_critical(shared_budget: u64, shared_used: u64) -> bool {
    shared_is_critical_for(shared_budget, shared_used, false)
}

/// Marks host memory pressured immediately (e.g. after an `E_OUTOFMEMORY` error).
pub(crate) fn force_pressure() {
    PRESSURED.store(true, Ordering::Relaxed);
}

/// Current `(available_physical_mib, available_commit_mib)` from Windows, if available.
pub(crate) fn snapshot_mib() -> (u64, u64) {
    memory_status().map_or((0, 0), |s| (s.ullAvailPhys >> 20, s.ullAvailPageFile >> 20))
}

/// Called by the once-per-second submission sweep. A failed sample keeps the last good state.
pub(crate) fn refresh() {
    let Some(status) = memory_status() else { return };
    let (physical, commit) = (status.ullAvailPhys, status.ullAvailPageFile);
    let video = crate::host::host().and_then(|h| h.video_memory());
    let [local_budget, local_used, shared_budget, shared_used] = video.unwrap_or([0; 4]);
    let mut headroom = physical.min(commit);
    if shared_budget > 0 && u128::from(shared_used) * 100 >= u128::from(shared_budget) * 65 {
        headroom = headroom.min(shared_budget.saturating_sub(shared_used));
    }
    let very_low = very_low_ram();
    HEADROOM.store(headroom, Ordering::Relaxed);
    SHARED_CRITICAL.store(shared_is_critical_for(shared_budget, shared_used, very_low), Ordering::Relaxed);
    let was = pressured();
    let pressure = next_pressure_with_shared_for(was, physical, commit, shared_budget, shared_used, very_low);
    if pressure != was {
        PRESSURED.store(pressure, Ordering::Relaxed);
        if pressure {
            tracing::warn!(
                pid = std::process::id(),
                frame = crate::frame::current(),
                physical_free_mib = physical >> 20,
                available_commit_mib = commit >> 20,
                local_used_mib = local_used >> 20,
                local_budget_mib = local_budget >> 20,
                shared_used_mib = shared_used >> 20,
                shared_budget_mib = shared_budget >> 20,
                shadow_live_mib = crate::shadow::total_live_bytes() >> 20,
                shadow_pool_mib = crate::shadow::snapshot_pool_bytes() >> 20,
                buffer_mirror_span_mib = crate::shadow::live_span_bytes() >> 20,
                texture_staging_mib = crate::texture::live_staging_bytes() >> 20,
                spare_scratch_mib = crate::host::ALIVE[4].load(Ordering::Relaxed) >> 20,
                game_filled_gpu_texture_mib = crate::resource::KINDS[1].load(Ordering::Relaxed) >> 20,
                "host memory pressure: limiting texture prefetch and idle upload storage"
            );
        } else {
            tracing::info!(
                physical_free_mib = physical >> 20,
                available_commit_mib = commit >> 20,
                shared_used_mib = shared_used >> 20,
                shared_budget_mib = shared_budget >> 20,
                "host memory pressure cleared"
            );
        }
    }
}

pub(crate) fn headroom() -> Option<u64> {
    let bytes = HEADROOM.load(Ordering::Relaxed);
    (bytes != u64::MAX).then_some(bytes)
}

pub(crate) fn pressured() -> bool {
    PRESSURED.load(Ordering::Relaxed)
}

/// True when OS commit headroom is critically low (< 1.5 GiB) or an allocation
/// just failed with `E_OUTOFMEMORY`, requiring emergency throttling.
pub(crate) fn critical_commit() -> bool {
    let now = crate::host::coarse_ms();
    now < RETRY_UPLOAD_AT.load(Ordering::Relaxed)
        || sample_commit(now) < 1536 * MIB
        || SHARED_CRITICAL.load(Ordering::Relaxed)
}

/// A soft per-frame admission limit. A single texture can cross it. The
/// queue retains unserved slots so they can be retried on later frames.
pub(crate) fn prefetch_limit_for(
    bytes: u64,
    pressure: bool,
    critical: bool,
    limited: bool,
    very_low: bool,
    staging: u64,
) -> u64 {
    let cap = if very_low { 512 * MIB } else { GIB };
    if critical {
        bytes.min(16 << 20)
    } else if pressure || (limited && staging >= cap) {
        bytes.min(64 << 20)
    } else {
        bytes
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn prefetch_limit(bytes: u64, pressure: bool, critical: bool, limited: bool, staging: u64) -> u64 {
    prefetch_limit_for(bytes, pressure, critical, limited, false, staging)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bake_and_lookahead_leave_room_for_live_game_allocations() {
        for (commit, bake, lookahead, growth) in [
            (128 * MIB, false, false, false),
            (256 * MIB, true, false, false),
            (512 * MIB, true, true, false),
            (768 * MIB, true, true, false),
            (1024 * MIB, true, true, true),
        ] {
            assert_eq!(has_upload_reserve(commit, true), bake);
            assert_eq!(has_upload_reserve(commit, false), lookahead);
            assert_eq!(has_growth_reserve(commit, false), growth);
        }
    }

    #[test]
    fn either_constraint_enters_pressure_and_both_must_recover() {
        assert!(next_pressure(false, 12 * GIB, 4 * GIB));
        assert!(next_pressure(false, 3 * GIB, 16 * GIB));
        assert!(next_pressure(true, 4 * GIB, 7 * GIB));
        assert!(next_pressure(true, 6 * GIB, 5 * GIB));
        assert!(!next_pressure(true, 5 * GIB, 6 * GIB));
        assert!(!next_pressure(false, 4 * GIB, 5 * GIB));
        // WDDM shared GPU memory pressure (e.g. 7.2 GiB on 16 GB laptop) enters at 72%, recovers below 64%, critical at 85%.
        assert!(next_pressure_with_shared(false, 8 * GIB, 16 * GIB, 1000 * MIB, 720 * MIB));
        assert!(!next_pressure_with_shared(false, 8 * GIB, 16 * GIB, 1000 * MIB, 719 * MIB));
        assert!(next_pressure_with_shared(true, 8 * GIB, 16 * GIB, 1000 * MIB, 640 * MIB));
        assert!(!next_pressure_with_shared(true, 8 * GIB, 16 * GIB, 1000 * MIB, 639 * MIB));
        assert!(shared_is_critical(1000 * MIB, 850 * MIB));
        assert!(!shared_is_critical(1000 * MIB, 849 * MIB));
        // On ~16 GB laptops (`very_low=true`), low Windows Standby-free physical RAM with ample pagefile commit
        // does not stall prefetch until critically low, and WDDM shared GPU spillover waits until 86% / 78% / 92%.
        assert!(!next_pressure_for(false, 320 * MIB, 14 * GIB, true));
        assert!(next_pressure_for(false, 384 * MIB, 5 * GIB, true));
        assert!(next_pressure_for(false, 128 * MIB, 24 * GIB, true));
        assert!(next_pressure_for(true, 512 * MIB, 5 * GIB, true));
        assert!(!next_pressure_for(true, 768 * MIB, 6 * GIB, true));
        assert!(!next_pressure_with_shared_for(false, 4 * GIB, 24 * GIB, 1000 * MIB, 750 * MIB, true));
        assert!(next_pressure_with_shared_for(false, 4 * GIB, 24 * GIB, 1000 * MIB, 860 * MIB, true));
        assert!(next_pressure_with_shared_for(true, 4 * GIB, 24 * GIB, 1000 * MIB, 780 * MIB, true));
        assert!(!next_pressure_with_shared_for(true, 4 * GIB, 24 * GIB, 1000 * MIB, 779 * MIB, true));
        assert!(!shared_is_critical_for(1000 * MIB, 850 * MIB, true));
        assert!(shared_is_critical_for(1000 * MIB, 920 * MIB, true));
    }

    #[test]
    fn prefetch_respects_existing_budgets_and_resumes_after_recovery() {
        assert_eq!(prefetch_limit(512 << 20, false, false, true, GIB), 64 << 20);
        assert_eq!(prefetch_limit(512 << 20, false, false, true, GIB - 1), 512 << 20);
        assert_eq!(prefetch_limit(512 << 20, false, false, false, 2 * GIB), 512 << 20);
        assert_eq!(prefetch_limit_for(512 << 20, false, false, true, true, 512 * MIB), 64 << 20);
        assert_eq!(prefetch_limit_for(512 << 20, false, false, true, true, 512 * MIB - 1), 512 << 20);
        assert_eq!(prefetch_limit(0, true, true, false, 0), 0);
        assert_eq!(prefetch_limit(8 << 20, true, true, false, 0), 8 << 20);
        assert_eq!(prefetch_limit(512 << 20, true, false, false, 0), 64 << 20);
        assert_eq!(prefetch_limit(512 << 20, true, true, false, 0), 16 << 20);
        assert_eq!(prefetch_limit(192 << 20, false, false, false, 0), 192 << 20);
    }
}
