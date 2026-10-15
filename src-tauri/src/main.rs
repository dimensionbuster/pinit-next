// Prevents an extra console window on Windows in release, DO NOT REMOVE!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    // The browser launches this same executable as its native messaging host.
    let args: Vec<String> = std::env::args().collect();
    if pinit_next_lib::native_host::is_native_host_launch(&args) {
        std::process::exit(pinit_next_lib::native_host::run());
    }
    pinit_next_lib::run()
}
