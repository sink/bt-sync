use bt_sync::*;
use bluetooth::*;
use utils::*;
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
