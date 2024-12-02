use nt_hive::{Hive, KeyValueData};
use regex::Regex;
use std::{collections::HashMap, process::Command};
use std::fs;
use std::io::Read;
use std::path::Path;
use anyhow::{Context, Ok, Result as AnyResult};
use term_ansi::*;

pub fn fmt_mac(mac: &str) -> String {
    mac.as_bytes()
       .chunks(2)
       .map(|chunk| std::str::from_utf8(chunk).unwrap().to_uppercase())
       .collect::<Vec<String>>()
       .join(":")
}

pub fn parse_reg(path: &str) -> AnyResult<HashMap<String, (String, String)>> {
    let mut file = fs::File::open(path).context("Failed to open hive")?;
    let mut buf = Vec::<u8>::new();
    file.read_to_end(&mut buf).context("Failed to read hive")?;

    let hive = Hive::new(buf.as_ref()).context("Failed to parse hive")?;
    let root = hive.root_key_node().unwrap();

    let mut bt_name_map = HashMap::new();
    
    let keys = root.subpath(r"ControlSet001\Services\BTHPORT\Parameters\Devices").unwrap().unwrap();
    if let Some(subs) = keys.subkeys() {
        for key in subs.context("Failed to get subkeys")? {
            let key = key.context("Failed to enumerate key")?;
            if let Some(val) = key.value("Name") {
                if let KeyValueData::Small(data) = val.context("failed to get binary data")?.data()? {
                    let s = std::str::from_utf8(&data[..data.iter().position(|&r| r==0).unwrap_or(data.len())]).unwrap();
                    bt_name_map.insert(key.name().context("failed to get name")?.to_string(), s.to_string());
                }
            }
        }
    }

    let mut bt_device_info = HashMap::new();

    let keys = root.subpath(r"ControlSet001\Services\BTHPORT\Parameters\Keys").unwrap().unwrap();
    if let Some(subkeys) = keys.subkeys() {
        for dev in subkeys.context("Failed to get subkeys")? {
            let dev = dev.context("Failed to enumerate key")?;
            if let Some(subs) = dev.subkeys() {
                for key in subs.context("Failed to get subkeys")? {
                    let key = key.context("Failed to enumerate key")?;

                    if let Some(val) = key.value("LTK") {
                        if let KeyValueData::Small(data) = val.context("Failed to get binary data")?.data()? {
                            let ltk = data.iter().map(|b| format!("{:02X}", b)).collect::<String>();
                            if let Some(bt_name) = bt_name_map.get(&key.name().context("Failed to get name")?.to_string()) {
                                bt_device_info.insert(bt_name.clone(), (fmt_mac(&key.name().context("Failed to get name")?.to_string()), ltk));
                            }
                        }
                    }
                }
            }
        }
    }

    println!("{}", green!("=== Get bluetooth info from {} ===", red!("{}", path)));

    println!("{} | {} | {}", blue!("{:<30}", "Device Name"), blue!("{:<20}", "Address"), blue!("{:<40} ", "Key"));
    println!("{}", "-".repeat(88));
    for (name, (mac, uuid)) in &bt_device_info {
        println!("{:<30} | {:<20} | {:<40}", name, mac, uuid);
    }

    Ok(bt_device_info)
}

fn find_and_mount_ntfs_partitions() -> AnyResult<HashMap<String, (String, String)>> {
    let mut bt_device_info:HashMap<String, (String, String)> = HashMap::new();

    let output = Command::new("lsblk")
        .args(["-o", "NAME,FSTYPE,MOUNTPOINT", "--pairs", "--noheadings"])
        .output()?;
    if !output.status.success() {
        return Ok(bt_device_info);
    }

    let re = Regex::new(r#"NAME="([^"]+)" FSTYPE="([^"]+)" MOUNTPOINT="([^"]*)""#).unwrap();
    for cap in re.captures_iter(&String::from_utf8_lossy(&output.stdout)) {
        let (name, fstype, mountpoint) = (cap[1].to_string(), cap[2].to_string(), cap[3].to_string());

        if fstype == "ntfs" {
            if mountpoint.is_empty() {
                let device = format!("/dev/{}", name);
                let mount_point = "/mnt/temp";
                fs::create_dir_all(mount_point)?;

                let mount_status = Command::new("mount").args(["-t", "ntfs3", &device, mount_point]).status()?;
                if mount_status.success() {
                    let reg_path = format!("{}/Windows/System32/config/SYSTEM", mount_point);
                    println!("name={}, mountpoint={}, reg_path={}", name, mount_point, reg_path);
                    if std::path::Path::new(&reg_path).exists() {
                        bt_device_info = parse_reg(&reg_path).context("Failed to parse registry")?;
                        if !bt_device_info.is_empty() {
                            let _ = Command::new("umount").arg(mount_point).status();
                            break;
                        }
                    }
                }
                let _ = Command::new("umount").arg(mount_point).status();
            } else {
                let reg_path = format!("{}/Windows/System32/config/SYSTEM", mountpoint);
                if std::path::Path::new(&reg_path).exists() {
                    let bt_device_info = parse_reg(&reg_path).context("Failed to parse registry")?;
                    if !bt_device_info.is_empty() {
                        break;
                    }
                }
            }
        }
    }

    Ok(bt_device_info)
}

pub fn update_ltk(c: &str, ltk: &str) -> String {
    let mut in_ltk = false;
    let mut updated = String::new();
    for line in c.lines() {
        if line.trim() == "[LongTermKey]" {
            in_ltk = true;
        }
        if in_ltk && line.starts_with("Key=") {
            updated.push_str(&format!("Key={}\n", ltk));
            in_ltk = false;
        } else {
            updated.push_str(line);
            updated.push('\n');
        }
    }
    updated
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

fn restart_bluetooth_service() {
    use std::process::Command;
    let output = Command::new("systemctl")
        .arg("restart")
        .arg("bluetooth")
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        println!("\n{}", green!("=== Bluetooth service restarted successfully. ==="));
    } else {
        eprintln!("\nFailed to restart Bluetooth service. Error: {}", String::from_utf8_lossy(&output.stderr));
    }
}

pub fn process_bth_device(path: std::path::PathBuf, bt_device_info: &HashMap<String, (String, String)>) -> AnyResult<()> {
    let mut result_map = HashMap::new();

    for sub_entry in fs::read_dir(&path)? {
        let sub_path = sub_entry?.path();
        if sub_path.is_dir() {
            let path = Path::new(sub_path.to_str().unwrap());
            if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
                if file_name.contains(':') {
                    let info_path = path.join("info");
                    let content = fs::read_to_string(&info_path)?;
                    if let Some(name) = Regex::new(r"(?m)^Name=(.*)$").unwrap().captures(&content).and_then(|caps| caps.get(1).map(|m| m.as_str().to_string())) {
                        let ltk_old = get_ltk(&content);
                        if let Some((mac, ltk)) = bt_device_info.get(&name) {
                            let new_content = update_ltk(&content, ltk);
                            fs::write(format!("{}", info_path.to_str().unwrap()), &new_content)?;
                            fs::rename(path, path.parent().unwrap().join(mac))?;
        
                            result_map.insert(name, (file_name.to_string(), mac, ltk_old, ltk.clone()));
                        }
                    }
                }
            }

        }
    }

    if result_map.is_empty() {
        println!("{}", green!("\n=== NO LINUX bluetooth info found from {} ===", red!("{}", path.to_string_lossy())));
        return Ok(())
    }

    println!("{}", green!("\n=== Set LINUX bluetooth info from {} ===", red!("{}", path.to_string_lossy())));

    println!("{} | {} | {}", blue!("{:<30}", "Device Name"), blue!("{:<29}", "Address"), blue!("{:<40} ", "Key"));
    println!("{}", "-".repeat(88));
    for (name, (old_mac, new_mac, old_ltk, new_ltk)) in &result_map {
        println!("{} | FROM {} | FROM {}", cyan!("{:<30}", name), yellow!("{:<24}", old_mac), yellow!("{:<40}", old_ltk));
        println!("{} |   TO {} |   TO {}", cyan!("{:<30}", ""), magenta!("{:<24}", new_mac), magenta!("{:<40}", new_ltk));
    }

    restart_bluetooth_service();
    Ok(())
}

pub fn list_ntfs_mounts() -> Vec<String> {
    use std::process::Command;
    let output = Command::new("mount").output().expect("Mount cmd failed");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 4 && ["ntfs", "ntfs3"].contains(&parts[4].to_lowercase().as_str()) {
                Some(parts[2].to_string())
            } else {
                None
            }
        })
        .collect()
}

pub fn process_bluetooth_devices(bt_dir_path: &str) -> AnyResult<()> {
    let bt_device_info = find_and_mount_ntfs_partitions().context("Failed to parse registry")?;
    if bt_device_info.is_empty() {
        eprintln!("No LTK to show.");
        return Ok(());
    }

    for entry in fs::read_dir(bt_dir_path)? {
        let path = entry?.path();
        if path.is_dir() {
            process_bth_device(path, &bt_device_info)?;
        }
    }
    
    Ok(())
}