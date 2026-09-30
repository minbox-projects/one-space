use super::{bump_retry_poke, SLEEP_RESUME_GAP_THRESHOLD, SLEEP_RESUME_HEARTBEAT_INTERVAL};
use std::thread::{self};
use std::time::SystemTime;

pub fn start_sleep_resume_monitor() {
    thread::spawn(move || {
        let mut last_seen = SystemTime::now();
        loop {
            thread::sleep(SLEEP_RESUME_HEARTBEAT_INTERVAL);
            let now = SystemTime::now();
            let elapsed = now
                .duration_since(last_seen)
                .unwrap_or(SLEEP_RESUME_HEARTBEAT_INTERVAL);
            last_seen = now;
            if elapsed >= SLEEP_RESUME_GAP_THRESHOLD {
                crate::app_runtime::mark_system_resume();
                bump_retry_poke();
            }
        }
    });
}

#[cfg(target_os = "macos")]
pub fn start_system_wake_observer() {
    use block2::RcBlock;
    use objc2_app_kit::{NSWorkspace, NSWorkspaceDidWakeNotification};
    use objc2_foundation::NSNotification;
    use std::ptr::NonNull;

    let workspace = NSWorkspace::sharedWorkspace();
    let center = workspace.notificationCenter();
    let block = RcBlock::new(move |_notification: NonNull<NSNotification>| {
        crate::app_runtime::mark_system_resume();
        bump_retry_poke();
    });
    let wake_notification = unsafe { NSWorkspaceDidWakeNotification };
    let observer = unsafe {
        center.addObserverForName_object_queue_usingBlock(
            Some(wake_notification),
            None,
            None,
            &block,
        )
    };

    // Keep the observer and block alive for the process lifetime.
    std::mem::forget(observer);
    std::mem::forget(block);
}

#[cfg(not(target_os = "macos"))]
pub fn start_system_wake_observer() {}
