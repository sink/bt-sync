use bt_sync::*;
use std::process;

fn main() {
    if !is_root() {
        eprintln!("This program must be run as root. Attempting to restart with sudo...");
        restart_with_sudo();
        return;
    }

    if let Err(e) = process_bluetooth_devices("/var/lib/bluetooth/") {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

fn is_root() -> bool {
    use std::process::Command;

    match Command::new("id").arg("-u").output() {
        Ok(output) => {
            if output.status.success() {
                let uid = String::from_utf8_lossy(&output.stdout).trim().parse::<u32>().unwrap_or(0);
                uid == 0
            } else {
                false
            }
        }
        Err(_) => false,
    }
}

fn restart_with_sudo() {
    use std::env;
    use std::process::Command;

    let current_exe = env::current_exe().expect("Failed to get current executable path");
    let args: Vec<String> = env::args().collect();

    let status = Command::new("sudo")
        .arg(current_exe)
        .args(&args[1..])
        .status()
        .expect("Failed to execute sudo");

    if !status.success() {
        eprintln!("Failed to restart with sudo. Please run the program manually with sudo.");
        process::exit(1);
    }
}