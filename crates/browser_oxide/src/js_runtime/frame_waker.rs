//! Per-frame waker for the frame-tree driver. Each isolate owns one
//! `FrameWaker` for its lifetime; the driver polls it with a `Waker` built from
//! this, which deno_core registers as the isolate's only wake target (timers,
//! ops, network). A completion sets `dirty` and wakes the driver, which re-polls
//! only the frames whose `dirty` bit is set. The `Arc` must be owned
//! persistently, never recreated per poll: deno_core keeps the waker from the
//! last poll, so a completion between polls must set `dirty` on the same object
//! the driver reads next sweep.

use futures_util::task::AtomicWaker;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::task::{Wake, Waker};

pub struct FrameWaker {
    dirty: AtomicBool,
    parent: AtomicWaker,
}

impl FrameWaker {
    /// Starts dirty so the frame is polled at least once, which registers the
    /// waker into the isolate's timer/op/network wakeups.
    pub fn new_dirty() -> Arc<Self> {
        Arc::new(Self {
            dirty: AtomicBool::new(true),
            parent: AtomicWaker::new(),
        })
    }

    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::Acquire)
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }

    /// Sets dirty without waking, for when the driver itself delivered a message
    /// and re-polls this frame in the same turn.
    pub fn mark_dirty(&self) {
        self.dirty.store(true, Ordering::Release);
    }

    pub fn register(&self, driver: &Waker) {
        self.parent.register(driver);
    }

    pub fn waker(self: &Arc<Self>) -> Waker {
        Waker::from(self.clone())
    }
}

impl Wake for FrameWaker {
    fn wake(self: Arc<Self>) {
        self.wake_by_ref();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.dirty.store(true, Ordering::Release);
        self.parent.wake();
    }
}
