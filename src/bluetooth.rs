use std::{collections::HashMap, fs, path::{Path, PathBuf}, process::Command};

use anyhow::Result;
use regex::Regex;
use term_ansi::*;

use crate::partitions::*;

#[derive(Debug, Clone, PartialEq)]
pub struct BtDeviceInfo {
    pub mac: String,
    pub ltk: String,
    pub erand: String,
    pub ediv: String
}

pub fn get_ltk(c: &str) -> String {
    let mut in_ltk = false;
    for line in c.lines() {
        if line.trim() == "[LongTermKey]" {
            in_ltk = true;
            continue;
        }
        if in_ltk && line.starts_with("Key=") {
            return line[4..].to_string();
        }
    }
    return "".to_string();
}

pub fn fmt_mac(mac: &str) -> String {
    mac.as_bytes()
       .chunks(2)
       .map(|chunk| std::str::from_utf8(chunk).unwrap().to_uppercase())
       .collect::<Vec<String>>()
       .join(":")
}

pub fn update_bt_info(c: &str, info: &BtDeviceInfo) -> String {
    let mut in_ltk = false;
    let mut updated = String::with_capacity(c.len() + 50);

    for line in c.lines() {
        if line.starts_with('[') {
            in_ltk = line == "[LongTermKey]";
        }

        if in_ltk {
            match line.splitn(2, '=').next() {
                Some("Key") => { updated.push_str(&format!("Key={}\n", info.ltk)); continue; }
                Some("EDiv") => { updated.push_str(&format!("EDiv={}\n", info.ediv)); continue; }
                Some("Rand") => { updated.push_str(&format!("Rand={}\n", info.erand)); continue; }
                _ => {}
            }
        }

        updated.push_str(line);
        updated.push('\n');
    }

    updated
}

fn restart_bluetooth_service() {
    if std::env::var("TESTING").is_ok() { return;}

    let output = Command::new("systemctl").args(["restart", "bluetooth"]).output().expect("Failed to execute command");

    if output.status.success() {
        println!("\n{}", green!("=== Bluetooth service restarted successfully. ==="));
    } else {
        eprintln!("\nFailed to restart Bluetooth service. Error: {}", String::from_utf8_lossy(&output.stderr));
    }
}

pub fn process_bth_device(path: PathBuf, bt_device_info: &HashMap<String, BtDeviceInfo>) -> Result<(), Box<dyn std::error::Error>> {
    let mut result_map = HashMap::new();
    let has_updates = process_directories(&path, bt_device_info, &mut result_map)?;

    if !has_updates {
        println!("\n=== NO Linux bluetooth info found from {} ===", path.display());
        return Ok(())
    }

    print_update_results(&result_map);
    restart_bluetooth_service();
    Ok(())
}

fn process_directories(
    path: &Path,
    bt_device_info: &HashMap<String, BtDeviceInfo>,
    result_map: &mut HashMap<String, (String, String, String, String)>
) -> Result<bool, Box<dyn std::error::Error>> {
    let mut has_updates = false;

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let sub_path = entry.path();

        if is_valid_device_directory(&sub_path) {
            if let Some((name, content)) = read_device_info(&sub_path)? {
                if let Some(info) = bt_device_info.get(&name) {
                    update_device_info(&name, &sub_path, &content, info, result_map)?;
                    has_updates = true;
                }
            }
        }
    }

    Ok(has_updates)
}

fn is_valid_device_directory(sub_path: &Path) -> bool {
    sub_path.is_dir() && sub_path.file_name().and_then(|f| f.to_str()).map_or(false, |name| name.contains(':'))
}

fn read_device_info(sub_path: &Path) -> Result<Option<(String, String)>, Box<dyn std::error::Error>> {
    let info_path = sub_path.join("info");
    let content = fs::read_to_string(&info_path)?;
    if let Some(captures) = Regex::new(r"(?m)^Name=(.*)$")?.captures(&content) {
        if let Some(name_match) = captures.get(1) {
            return Ok(Some((name_match.as_str().to_string(), content)));
        }
    }
    Ok(None)
}

fn update_device_info(
    name: &str,
    sub_path: &Path,
    content: &str,
    info: &BtDeviceInfo,
    result_map: &mut HashMap<String, (String, String, String, String)>
) -> Result<(), Box<dyn std::error::Error>> {
    let new_content = update_bt_info(content, info);
    fs::write(sub_path.join("info"), &new_content)?;
    fs::rename(sub_path, sub_path.parent().unwrap().join(&info.mac))?;

    result_map.insert(
        name.to_string(),
        (
            sub_path.file_name().unwrap().to_string_lossy().into_owned(),
            info.mac.clone(),
            get_ltk(content),
            info.ltk.clone()
        )
    );

    Ok(())
}

fn print_update_results(result_map: &HashMap<String, (String, String, String, String)>) {
    println!("{}", green!("\n=== Update Linux bluetooth info ==="));

    println!("{} |      {} |      {}", 
        blue!("{:<30}", "Device Name"),
        blue!("{:<24}", "Address"),
        blue!("{:<40}", "Key"));
    println!("{}", "-".repeat(102));
    
    for (name, (old_mac, new_mac, old_ltk, new_ltk)) in result_map {
        let name_colored = rgb!(0xf0, 0x00, 0x56, "{:<30}", name);
        let old_mac_colored = rgb!(0xaa, 0x96, 0xda, "{:<24}", old_mac);
        let old_ltk_colored = rgb!(0xaa, 0x96, 0xda, "{:<40}", old_ltk);
        println!("{} | FROM {} | FROM {}", name_colored, old_mac_colored, old_ltk_colored);

        let space_colored = rgb!(0xf0, 0x00, 0x56, "{:<30}", " ");
        let new_mac_colored = rgb!(0x00, 0xe0, 0x79, "{:<24}", new_mac);
        let new_ltk_colored = rgb!(0x00, 0xe0, 0x79, "{:<40}", new_ltk);
        println!("{} |   TO {} |   TO {}", space_colored, new_mac_colored, new_ltk_colored);
    }
}

pub fn process_bluetooth_devices(bt_dir_path: &str) -> Result<()> {
    let bt_device_info = find_and_mount_ntfs_partitions()?;
    if bt_device_info.is_empty() {
        eprintln!("No LTK to show.");
        return Ok(());
    }

    for entry in fs::read_dir(bt_dir_path)? {
        let path = entry?.path();
        if path.is_dir() {
            let _ = process_bth_device(path, &bt_device_info);
        }
    }
    
    Ok(())
}