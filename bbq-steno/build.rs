//! Include revision information and time into the build.
//!
//! This really only needs to happen when building the app, but doesn't
//! seem to hurt anything to have it run always.

fn main() {
    build_data::set_GIT_COMMIT();
    build_data::set_GIT_DIRTY();
    build_data::set_BUILD_TIMESTAMP();
}
