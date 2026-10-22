//! The global allocator that counts what a program allocates.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether the counting allocator has allocated in this process.
static INSTALLED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The allocations and the requested bytes of this thread. A constant
    /// initializer and no destructor keep a read of it from allocating.
    static THREAD: Cell<(u64, u64)> = const { Cell::new((0, 0)) };
}

/// A global allocator that counts the allocations and the requested bytes of
/// each thread, and hands every request to the system allocator.
///
/// The allocation ceilings read its counts, so a test binary that states a
/// ceiling installs it:
///
/// ```
/// use dokimi_assert::bench::CountingAllocator;
///
/// #[global_allocator]
/// static ALLOCATOR: CountingAllocator = CountingAllocator::new();
/// ```
///
/// A ceiling counts the allocations of the threads that run its body, so the
/// tests that run beside it on other threads do not change its count. A
/// program has one global allocator and chooses it, so the library cannot
/// install this one on a caller's behalf. Without it a ceiling ends with a
/// fault that states that nothing counts allocations.
#[derive(Debug, Default, Clone, Copy)]
pub struct CountingAllocator;

impl CountingAllocator {
    /// Returns the allocator.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

fn count(bytes: usize) {
    INSTALLED.store(true, Ordering::Relaxed);
    let bytes = u64::try_from(bytes).unwrap_or(u64::MAX);
    // A thread can allocate while its storage is torn down, and counting
    // must never be why an allocation fails.
    let _ = THREAD.try_with(|held| {
        let (allocations, total) = held.get();
        held.set((allocations + 1, total.saturating_add(bytes)));
    });
}

// Each method hands the request to the system allocator unchanged and adds
// a count, so it upholds every rule that System upholds.
#[expect(unsafe_code, reason = "GlobalAlloc is an unsafe trait")]
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: the caller's layout goes to System unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        count(layout.size());
        // SAFETY: the caller's layout goes to System unchanged.
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: the pointer and its layout come from System.
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        count(new_size.saturating_sub(layout.size()));
        // SAFETY: the pointer and its layout come from System.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

/// Returns the allocations of the calling thread, or `None` when the
/// counting allocator is not the global allocator.
pub(crate) fn thread_allocations() -> Option<u64> {
    thread_counts().map(|(allocations, _)| allocations)
}

/// Returns the allocations and the requested bytes of the calling thread, or
/// `None` when the counting allocator is not the global allocator.
pub(crate) fn thread_counts() -> Option<(u64, u64)> {
    if !INSTALLED.load(Ordering::Relaxed) {
        return None;
    }
    THREAD.try_with(Cell::get).ok()
}
