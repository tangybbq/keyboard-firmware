//! Various structure checks.

use super::sync::k_mutex;
use crate::error;

macro_rules! check {
    ($name:ident) => {
        compile_error!("concat_ident is unsable, TODO");
    };
    ($rust_name:ident, $c_name:ident) => {
        let rust_size = core::mem::size_of::<$rust_name>();
        let c_size = unsafe {
            extern "C" {
                static $c_name: usize;
            }
            $c_name
        };
        if rust_size != c_size {
            error!(concat!("Size mismatch on ",
                           stringify!($rust_name),
                           ": rust:{}, c:{}"),
                   rust_size,
                   c_size);
            panic!("Size mismatch");
        }
    };
}

pub fn check_sizes() {
    check!(k_mutex, struct_k_mutex_size);
}
