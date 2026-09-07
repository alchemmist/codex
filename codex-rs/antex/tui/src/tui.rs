use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use std::time::Instant;

use tokio::sync::Notify;

#[derive(Clone, Debug)]
pub(crate) struct FrameRequester(Arc<FrameClock>);

#[derive(Debug)]
struct FrameClock {
    next: Mutex<Option<Instant>>,
    changed: Notify,
}

impl FrameRequester {
    pub(crate) fn new() -> Self {
        Self(Arc::new(FrameClock {
            next: Mutex::new(None),
            changed: Notify::new(),
        }))
    }

    pub(crate) fn schedule_frame_in(&self, delay: Duration) {
        let deadline = Instant::now() + delay;
        let mut next = self
            .0
            .next
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if next.is_none_or(|current| deadline < current) {
            *next = Some(deadline);
            self.0.changed.notify_one();
        }
    }

    pub(crate) async fn next_frame(&self) {
        loop {
            let changed = self.0.changed.notified();
            let next = *self
                .0
                .next
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Some(deadline) = next {
                tokio::select! {
                    _=tokio::time::sleep_until(deadline.into())=>{
                        let mut next=self.0.next.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                        if next.is_some_and(|deadline|deadline<=Instant::now()) {*next=None;return;}
                    }
                    _=changed=>{},
                }
            } else {
                changed.await;
            }
        }
    }
}
