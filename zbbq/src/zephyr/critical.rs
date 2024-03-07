//! Zephyr implementation of critical section.

use core::ffi::c_int;

use critical_section::RawRestoreState;

struct ZephyrCriticalSection;
critical_section::set_impl!(ZephyrCriticalSection);

extern "C" {
    fn z_crit_acquire() -> c_int;
    fn z_crit_release(token: c_int);
}

unsafe impl critical_section::Impl for ZephyrCriticalSection {
    unsafe fn acquire() -> RawRestoreState {
        z_crit_acquire() as RawRestoreState
    }

    unsafe fn release(token: RawRestoreState) {
        z_crit_release(token as c_int);
    }
}
