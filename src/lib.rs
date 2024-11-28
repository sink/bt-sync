// src/lib.rs

use nt_hive::{Hive, KeyValueData, KeyValueDataType};
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::io::Read;
use std::path::Path;
use anyhow::{Context, Result as AnyResult};

pub fn fmt_mac(mac: &str) -> String {
    (0..mac.len())
        .step_by(2)
        .map(|i| &mac[i..i + 2])
        .map(|s| s.to_uppercase())
        .collect::<Vec<String>>()
        .join(":")
}

pub fn parse_reg(path: &str) -> AnyResult<HashMap<String, String>> {
    let mut file = fs::File::open(path).context("Failed to open hive")?;
    let mut buf = Vec::<u8>::new();
    file.read_to_end(&mut buf).context("Failed to read hive")?;

    let hive = Hive::new(buf.as_ref()).context("Failed to parse hive")?;
    let root = hive.root_key_node().unwrap();
    let keys = root.subpath(r"ControlSet001\Services\BTHPORT\Parameters\Keys").unwrap().unwrap();

    let mut ltk_map = HashMap::new();

    if let Some(subkeys) = keys.subkeys() {
        for dev in subkeys.context("Failed to get subkeys")? {
            let dev = dev.context("Failed to enumerate key")?;
            if let Some(subs) = dev.subkeys() {
                for key in subs.context("Failed to get subkeys")? {
                    let key = key.context("Failed to enumerate key")?;
                    let name = key.name().context("Failed to get name")?;
                    println!("BT name: {}", name);

                    if let Some(val) = key.value("LTK") {
                        let val = val.unwrap();
                        let vtype = val.data_type().context("Failed to get type")?;
                        if vtype != KeyValueDataType::RegBinary {
                            continue;
                        }

                        match val.data().context("Failed to get binary data")? {
                            KeyValueData::Small(data) => {
                                let ltk = data.iter().map(|b| format!("{:02X}", b)).collect::<String>();
                                let mac = fmt_mac(&name.to_string());
                                ltk_map.insert(mac, ltk);
                            },
                            KeyValueData::Big(_) => println!("BIG DATA"),
                        }
                    }
                }
            }
        }
    }

    Ok(ltk_map)
}

pub fn update_ltk(c: &str, ltk: &str) -> String {
    let mut in_ltk = false;
    let ends_nl = c.ends_with('\n');
    let updated = c.lines().map(|l| {
        if l.trim() == "[LongTermKey]" {
            in_ltk = true;
            l.to_string()
        } else if in_ltk && l.starts_with("Key=") {
            format!("Key={}", ltk)
        } else {
            l.to_string()
        }
    }).collect::<Vec<_>>().join("\n");
    if ends_nl && !updated.ends_with('\n') {
        updated + "\n"
    } else {
        updated
    }
}

fn process_bth_device(bt_path: &str, map: &HashMap<String, String>) -> io::Result<()> {
    let path = Path::new(bt_path);
    let name_re = Regex::new(r"(?m)^Name=(.*)$").unwrap();

    if let Some(file_name) = path.file_name().and_then(|f| f.to_str()) {
        if !file_name.contains(':') {
            return Ok(());
        }

        let info_path = path.join("info");
        let content = fs::read_to_string(&info_path)?;

        if let Some(caps) = name_re.captures(&content) {
            let name = caps.get(1).map_or("", |m| m.as_str());
            println!("Proc device: {}, Name: {}", file_name, name);

            for (mac, ltk) in map {
                if file_name.starts_with(&mac[..8]) {
                    println!("  Update LTK: {}", ltk);
                    let new_content = update_ltk(&content, ltk);

                    let updated_path = format!("{}.updated", info_path.to_str().unwrap());
                    fs::write(updated_path, &new_content)?;

                    let new_name = mac.clone();
                    let new_path = path.parent().unwrap().join(&new_name);
                    if new_path != *path {
                        match fs::rename(path, new_path) {
                            Ok(_) => println!("  Renamed from {} to {}", file_name, &new_name),
                            Err(e) => eprintln!("Failed to rename folder: {}", e),
                        }
                    }
                    break;
                }
            }
        }
    }

    Ok(())
}

pub fn list_ntfs_mounts() -> Vec<(String, String)> {
    use std::process::Command;
    let output = Command::new("mount").output().expect("Mount cmd failed");
    let binding = String::from_utf8_lossy(&output.stdout);
    let lines = binding.lines();
    lines.filter_map(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() > 4 && ["ntfs", "ntfs3"].contains(&parts[4].to_lowercase().as_str()) {
            Some((parts[0].to_string(), parts[2].to_string()))
        } else {
            None
        }
    }).collect()
}

pub fn process_bluetooth_devices(bt_dir_path: &str) -> AnyResult<()> {
    let mounts = list_ntfs_mounts();
    if mounts.is_empty() {
        return Err(anyhow::anyhow!("No NTFS mounts found."));
    }

    let mut ltk_map = HashMap::new();
    for (_device, mount) in mounts {
        let reg_path = format!("{}/Windows/System32/config/SYSTEM", mount);
        if !std::path::Path::new(&reg_path).exists() {
            continue;
        }

        ltk_map = parse_reg(&reg_path).context("Failed to parse registry")?;
        if !ltk_map.is_empty() {
            println!("Parsed LTK:");
            for (mac, ltk) in &ltk_map {
                println!("{} = {}", mac, ltk);
            }
            break;
        } else {
            eprintln!("No LTK to show.");
        }
    }

    let bt_dir = Path::new(bt_dir_path);
    if let Err(_e) = fs::read_dir(bt_dir) {
        return Err(anyhow::anyhow!("Bluetooth directory not found at: {}", bt_dir_path));
    }

    for entry in fs::read_dir(bt_dir)? {
        let entry = entry.context("Failed to read directory entry")?;
        let path = entry.path();

        if path.is_dir() {
            for sub_entry in fs::read_dir(&path)? {
                let sub_entry = sub_entry.context("Failed to read subdirectory entry")?;
                let sub_path = sub_entry.path();

                if sub_path.is_dir() {
                    process_bth_device(sub_path.to_str().unwrap(), &ltk_map).context("Failed to process device")?;
                }
            }
        }
    }

    Ok(())
}