#[path = "../../src-tauri/native_runtime.rs"]
mod native_runtime;

fn main() {
    native_runtime::stage("Bard");
}
