// Include revision information and time into the build.

fn main() {
    build_data::set_GIT_COMMIT();
    build_data::set_GIT_DIRTY();
    build_data::set_SOURCE_TIMESTAMP();
}
