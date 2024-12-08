use bluetooth::process_bluetooth_devices;
use bt_sync::*;
use utils::{is_root, print_colored_ascii, restart_with_sudo};
use std::process;

fn main() {
    if !is_root() {
        restart_with_sudo();
        return;
    }

    print_colored_ascii();

    if let Err(e) = process_bluetooth_devices("/var/lib/bluetooth/") {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}
