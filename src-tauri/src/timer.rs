//! Event-driven waiting, with no polling.
//!
//! The scheduler thread blocks in a single `WaitForMultipleObjects` call until one of three
//! things happens:
//!
//! * the **waitable timer** is due - armed with an absolute wall-clock time for the next event;
//! * **settings changed** - `lib.rs` sets this event so an edited schedule is picked up at once
//!   instead of after the previously armed sleep;
//! * the machine **resumed from sleep** - Windows calls back in, which sets the third event, so
//!   a deadline that passed while suspended is reconciled immediately rather than late.
//!
//! An idle app therefore has one blocked thread and one timer: no tick, no interval, no CPU.
//!
//! The sleep/resume callback matters because Windows timer progress stops while the system is
//! suspended; the wake-up is what makes "slept through 05:00, resumed at 07:30" correct.

use std::io;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::ptr;

use chrono::{DateTime, Local, Utc};

type Handle = *mut std::ffi::c_void;

// kernel32
#[link(name = "kernel32")]
extern "system" {
    fn CreateWaitableTimerW(attributes: *mut Handle, manual_reset: i32, name: *const u16)
        -> Handle;
    fn SetWaitableTimer(
        timer: Handle,
        due_time: *const i64,
        period: i32,
        completion: *mut Handle,
        argument: *mut Handle,
        resume: i32,
    ) -> i32;
    fn CreateEventW(
        attributes: *mut Handle,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> Handle;
    fn SetEvent(event: Handle) -> i32;
    fn CancelWaitableTimer(timer: Handle) -> i32;
    fn WaitForMultipleObjects(
        count: u32,
        handles: *const Handle,
        wait_all: i32,
        milliseconds: u32,
    ) -> u32;
}

// powrprof
#[link(name = "powrprof")]
extern "system" {
    /// Three parameters: `Flags`, the `DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS` pointer, and an
    /// out-parameter that receives the registration handle the return value reports on.
    fn PowerRegisterSuspendResumeNotification(
        flags: u32,
        recipient: *mut std::ffi::c_void,
        registration: *mut Handle,
    ) -> u32;
    fn PowerUnregisterSuspendResumeNotification(registration: Handle) -> u32;
}

/// `DEVICE_NOTIFY_CALLBACK`: `Recipient` points at `DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS`.
const DEVICE_NOTIFY_CALLBACK: u32 = 2;
/// `PBT_APMRESUMEAUTOMATIC` and `PBT_APMRESUMESUSPEND`.
const PBT_APMRESUMEAUTOMATIC: u32 = 0x0012;
const PBT_APMRESUMESUSPEND: u32 = 0x0007;

const INFINITE: u32 = 0xFFFF_FFFF;
const WAIT_OBJECT_0: u32 = 0;
const WAIT_FAILED: u32 = 0xFFFF_FFFF;

/// 100-nanosecond intervals between 1601-01-01 and 1970-01-01, as used by `FILETIME`.
const EPOCH_OFFSET_INTERVALS: i64 = 11_644_473_600 * 10_000_000;

#[repr(C)]
struct DeviceNotifySubscribeParameters {
    callback: extern "system" fn(*mut std::ffi::c_void, u32, *mut std::ffi::c_void) -> u32,
    context: *mut std::ffi::c_void,
}

/// Why [`Waiter::wait`] returned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WakeReason {
    /// The next scheduled moment arrived.
    TimeElapsed,
    /// The schedules were edited; recompute.
    SettingsChanged,
    /// The machine came back from sleep; reconcile against the current clock.
    Resumed,
}

/// One blocked thread's worth of waiting, shared between the scheduler thread and the UI.
pub struct Waiter {
    timer: OwnedHandle,
    changed: OwnedHandle,
    resumed: OwnedHandle,
    /// Boxed so its address stays put: the power registration holds a pointer to it.
    _parameters: Box<DeviceNotifySubscribeParameters>,
    /// Registration from `PowerRegisterSuspendResumeNotification`, unregistered on drop.
    notification: Option<Handle>,
}

// SAFETY: Windows kernel handles are process-wide and every call made through them
// (SetWaitableTimer, SetEvent, WaitForMultipleObjects) is safe from any thread. The power
// callback only calls SetEvent on `resumed`, which is thread-safe.
unsafe impl Send for Waiter {}
unsafe impl Sync for Waiter {}

impl Waiter {
    /// Creates the timer, the two signalling events, and registers for resume notifications.
    ///
    /// Registration is best-effort: if Windows refuses it, waiting still works and the app just
    /// notices a resume when the timer fires.
    pub fn new() -> io::Result<Self> {
        Self {
            timer: new_handle(unsafe { CreateWaitableTimerW(ptr::null_mut(), 0, ptr::null()) })?,
            changed: new_handle(unsafe { CreateEventW(ptr::null_mut(), 0, 0, ptr::null()) })?,
            resumed: new_handle(unsafe { CreateEventW(ptr::null_mut(), 0, 0, ptr::null()) })?,
            _parameters: Box::new(DeviceNotifySubscribeParameters {
                callback: on_power_event,
                context: ptr::null_mut(),
            }),
            notification: None,
        }
        .with_power_notification()
    }

    fn with_power_notification(mut self) -> io::Result<Self> {
        self._parameters.context = self.resumed.as_raw_handle();
        let mut registration: Handle = ptr::null_mut();
        let status = unsafe {
            PowerRegisterSuspendResumeNotification(
                DEVICE_NOTIFY_CALLBACK,
                &mut *self._parameters as *mut DeviceNotifySubscribeParameters
                    as *mut std::ffi::c_void,
                &mut registration,
            )
        };
        // ERROR_SUCCESS plus a real handle: resume notifications are armed.
        if status == 0 && !registration.is_null() {
            self.notification = Some(registration);
        }
        Ok(self)
    }

    /// Arms the timer for an absolute local time. A time in the past fires immediately.
    pub fn arm(&self, at: DateTime<Local>) -> io::Result<()> {
        let due = to_filetime_intervals(at);
        let result = unsafe {
            SetWaitableTimer(
                self.timer.as_raw_handle(),
                &due,
                0,
                ptr::null_mut(),
                ptr::null_mut(),
                0, // do not wake the machine from sleep; reconcile on resume instead
            )
        };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Signals the scheduler that the schedules changed. Safe to call from any thread.
    pub fn notify_settings_changed(&self) {
        unsafe { SetEvent(self.changed.as_raw_handle()) };
    }

    /// Cancels the timer so nothing is scheduled; it stays quiet until armed again. Used when the
    /// last schedule is deleted, so a stale wake-up cannot arrive.
    ///
    /// `CancelWaitableTimer` rather than `SetWaitableTimer` with a null due time: the latter is
    /// rejected with ERROR_INVALID_PARAMETER (87) and would leave the old due time in place.
    pub fn cancel(&self) -> io::Result<()> {
        // SAFETY: the handle belongs to this struct and the call takes no other parameters.
        let result = unsafe { CancelWaitableTimer(self.timer.as_raw_handle()) };
        if result == 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Blocks until the timer is due, settings change, or the machine resumes.
    pub fn wait(&self) -> io::Result<WakeReason> {
        let handles = [
            self.timer.as_raw_handle(),
            self.changed.as_raw_handle(),
            self.resumed.as_raw_handle(),
        ];
        let result = unsafe { WaitForMultipleObjects(3, handles.as_ptr(), 0, INFINITE) };
        match result {
            WAIT_OBJECT_0 => Ok(WakeReason::TimeElapsed),
            value if value == WAIT_OBJECT_0 + 1 => Ok(WakeReason::SettingsChanged),
            value if value == WAIT_OBJECT_0 + 2 => Ok(WakeReason::Resumed),
            WAIT_FAILED => Err(io::Error::last_os_error()),
            other => Err(io::Error::other(format!(
                "unexpected WaitForMultipleObjects result {other}"
            ))),
        }
    }
}

impl Drop for Waiter {
    fn drop(&mut self) {
        if let Some(registration) = self.notification.take() {
            unsafe { PowerUnregisterSuspendResumeNotification(registration) };
        }
    }
}

/// Runs on a system thread when the machine suspends or resumes.
extern "system" fn on_power_event(
    context: *mut std::ffi::c_void,
    event_type: u32,
    _setting: *mut std::ffi::c_void,
) -> u32 {
    if event_type == PBT_APMRESUMEAUTOMATIC || event_type == PBT_APMRESUMESUSPEND {
        // `context` is the resumed event handle, which outlives the registration.
        unsafe { SetEvent(context) };
    }
    0
}

fn new_handle(raw: Handle) -> io::Result<OwnedHandle> {
    if raw.is_null() {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: `raw` is a fresh, valid handle we own and nothing else references.
    Ok(unsafe { OwnedHandle::from_raw_handle(raw) })
}

/// Converts a local time to the absolute 100-nanosecond interval count since 1601-01-01 UTC.
fn to_filetime_intervals(at: DateTime<Local>) -> i64 {
    let utc: DateTime<Utc> = at.with_timezone(&Utc);
    utc.timestamp() * 10_000_000
        + i64::from(utc.timestamp_subsec_nanos()) / 100
        + EPOCH_OFFSET_INTERVALS
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;
    use std::time::{Duration, Instant};

    #[test]
    fn arms_for_an_absolute_time_and_wakes() {
        let waiter = Waiter::new().expect("create waiter");
        let started = Instant::now();

        waiter
            .arm(Local::now() + TimeDelta::milliseconds(300))
            .expect("arm");
        assert_eq!(waiter.wait().expect("wait"), WakeReason::TimeElapsed);

        let elapsed = started.elapsed();
        assert!(
            elapsed >= Duration::from_millis(250),
            "woke too early: {elapsed:?}"
        );
        assert!(
            elapsed < Duration::from_secs(5),
            "woke too late: {elapsed:?}"
        );
    }

    #[test]
    fn a_due_time_in_the_past_fires_at_once() {
        let waiter = Waiter::new().expect("create waiter");
        waiter
            .arm(Local::now() - TimeDelta::minutes(5))
            .expect("arm");

        let started = Instant::now();
        assert_eq!(waiter.wait().expect("wait"), WakeReason::TimeElapsed);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn editing_schedules_interrupts_a_long_wait() {
        use std::sync::Arc;
        use std::thread;

        let waiter = Arc::new(Waiter::new().expect("create waiter"));
        // A wait far in the future: only the settings event can end it.
        waiter.arm(Local::now() + TimeDelta::hours(6)).expect("arm");

        let notifier = Arc::clone(&waiter);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(100));
            notifier.notify_settings_changed();
        });

        let started = Instant::now();
        assert_eq!(waiter.wait().expect("wait"), WakeReason::SettingsChanged);
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "the settings event did not interrupt the wait"
        );
    }

    #[test]
    fn registers_for_resume_notifications() {
        let waiter = Waiter::new().expect("create waiter");
        assert!(
            waiter.notification.is_some(),
            "PowerRegisterSuspendResumeNotification failed; resume handling would be degraded"
        );
    }

    #[test]
    fn cancelling_is_accepted_with_and_without_a_pending_time() {
        // Regression: cancelling via SetWaitableTimer with a null due time fails with error 87.
        let waiter = Waiter::new().expect("create waiter");

        waiter
            .cancel()
            .expect("cancelling a timer that was never armed");
        waiter
            .arm(Local::now() + TimeDelta::minutes(5))
            .expect("arm");
        waiter.cancel().expect("cancelling a pending timer");
    }

    #[test]
    fn filetime_conversion_matches_known_values() {
        // 1970-01-01 UTC is exactly the epoch offset, and one second is 10 million intervals.
        let epoch = DateTime::from_timestamp(0, 0)
            .expect("epoch")
            .with_timezone(&Utc);
        let local: DateTime<Local> = epoch.into();
        assert_eq!(to_filetime_intervals(local), EPOCH_OFFSET_INTERVALS);

        let later = DateTime::from_timestamp(1, 500_000_000)
            .expect("timestamp")
            .with_timezone(&Utc);
        let later: DateTime<Local> = later.into();
        assert_eq!(
            to_filetime_intervals(later),
            EPOCH_OFFSET_INTERVALS + 15_000_000
        );
    }
}
