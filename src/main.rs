use bluetooth::process_bluetooth_devices;
use bt_sync::*;
use std::process;
use term_ansi::*;

fn print_colored_ascii() {
    let ascii_art = r#"
  888888b. 88888888888                   .d8888b. Y88b   d88P 888b    888  .d8888b.  
  888  "88b    888                      d88P  Y88b Y88b d88P  8888b   888 d88P  Y88b 
  888  .88P    888                      Y88b.       Y88o88P   88888b  888 888    888 
  8888888K.    888                       "Y888b.     Y888P    888Y88b 888 888        
  888  "Y88b   888          888888          "Y88b.    888     888 Y88b888 888        
  888    888   888                            "888    888     888  Y88888 888    888 
  888   d88P   888                      Y88b  d88P    888     888   Y8888 Y88b  d88P 
  8888888P"    888                       "Y8888P"     888     888    Y888  "Y8888P"  "#;

    let global_start_color = (255, 0, 0);
    let global_end_color = (0, 0, 255);

    for (line_index, line) in ascii_art.lines().enumerate() {
        print!("        ");
        for (col_index, c) in line.chars().enumerate() {
            let row_factor = line_index as f32 / ascii_art.lines().count() as f32; 
            let col_factor = col_index as f32 / line.len() as f32;  
            let factor = (row_factor + col_factor) / 2.0;  

            fn lerp(a: f32, b: f32, t: f32) -> f32 {
                a + t * (b - a)
            }

            let color = (
                lerp(global_start_color.0 as f32, global_end_color.0 as f32, factor) as u8,
                lerp(global_start_color.1 as f32, global_end_color.1 as f32, factor) as u8,
                lerp(global_start_color.2 as f32, global_end_color.2 as f32, factor) as u8,
            );

            print!("{}", rgb!(color.0, color.1, color.2, "{}", c.to_string()));
        }
        println!();
    }
    println!();
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

    Command::new("sudo")
        .arg(current_exe)
        .args(&args[1..])
        .status()
        .expect("Failed to execute sudo");
}

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
