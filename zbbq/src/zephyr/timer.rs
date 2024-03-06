//! Zephyr timer support.

extern "C" {
    fn k_timer_init(timer: *mut struct_timer,
                    expiry_fn: Option<KTimerExpiry>,
                    stop_fn: Option<KTimerStop>);
    fn sys_k_timer_start(timer: *mut struct_timer,
                         duration: struct_k_timeout,
                         period: struct_k_timeout);
    fn sys_k_timer_stop(timer: *mut struct_timer);
    fn sys_k_timer_status_sync(timer: *mut struct_timer) -> u32;
}

type KTimerExpiry = extern "C" fn (timer: *mut struct_timer);
type KTimerStop = extern "C" fn (timer: *mut struct_timer);

// The internal timer structure.  This is opaque.
#[allow(non_camel_case_types)]
#[repr(transparent)]
pub struct struct_timer {
    _pad: [u8; 56],
}

#[cfg(CONFIG_TIMEOUT_64BIT)]
#[allow(non_camel_case_types)]
type k_ticks_t = u64;
#[cfg(not(CONFIG_TIMEOUT_64BIT))]
#[allow(non_camel_case_types)]
type k_ticks_t = u32;

#[allow(non_camel_case_types)]
#[derive(Copy, Clone)]
#[repr(C)]
struct struct_k_timeout {
    #[allow(dead_code)]
    ticks: k_ticks_t,
}

pub struct Timer {
    sys: *mut struct_timer,
}

impl Timer {
    pub unsafe fn new_from_c(timer: *mut struct_timer) -> Timer {
        k_timer_init(timer, None, None);
        Timer {sys: timer}
    }

    /// Start this timer firing at the given interval (in ms).
    pub fn start(&mut self, interval: u64) {
        let ticks = struct_k_timeout { ticks: interval * 10 };
        unsafe {
            sys_k_timer_start(self.sys, ticks, ticks);
        }
    }

    /// Stop this timer from running.
    #[allow(dead_code)]
    pub fn stop(&mut self) {
        unsafe {
            sys_k_timer_stop(self.sys);
        }
    }

    /// Wait until this timer has passed.
    pub fn wait(&mut self) {
        unsafe {
            sys_k_timer_status_sync(self.sys);
        }
    }
}
