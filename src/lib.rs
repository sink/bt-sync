use nt_hive::{Hive, KeyValueData};
use regex::Regex;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::HashMap, process::Command};
use std::fs;
use std::io::Read;
use std::path::Path;
use anyhow::{Context, Ok, Result as AnyResult};
use term_ansi::*;
use rand::Rng;

#[derive(Debug, Clone, PartialEq)]
pub struct BtDeviceInfo {
    pub mac: String,
    pub ltk: String,
    pub erand: String,
    pub edev: String
}

pub fn fmt_mac(mac: &str) -> String {
    mac.as_bytes()
       .chunks(2)
       .map(|chunk| std::str::from_utf8(chunk).unwrap().to_uppercase())
       .collect::<Vec<String>>()
       .join(":")
}

pub fn parse_reg(device: &str, mountpoint: &str) -> AnyResult<HashMap<String, BtDeviceInfo>> {
    let path = format!("{}/Windows/System32/config/SYSTEM", mountpoint);
    if !std::path::Path::new(&path).exists() {
        return Ok(HashMap::new());
    }

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

                    let mut ltk = String::new();
                    let mut erand: u64 = 0;
                    let mut edev: u32 = 0;

                    if let Some(val) = key.value("LTK") {
                        if let KeyValueData::Small(data) = val.context("Failed to get binary data")?.data()? {
                            ltk = data.iter().map(|b| format!("{:02X}", b)).collect::<String>();
                        }
                    }

                    if let Some(val) = key.value("ERand") {
                        erand = val?.qword_data().context(format!("Error getting ERand data"))?;
                    }

                    if let Some(val) = key.value("EDIV") {
                        edev = val?.dword_data().context(format!("Error getting EDIV data"))?;
                    }

                    if !ltk.is_empty() {
                        if let Some(bt_name) = bt_name_map.get(&key.name().context("Failed to get name")?.to_string()) {
                            bt_device_info.insert(bt_name.clone(), BtDeviceInfo {
                                mac: fmt_mac(&key.name().context("Failed to get name")?.to_string()),
                                ltk: ltk,
                                erand: format!("{}", erand),
                                edev: format!("{}", edev),
                            });
                        }
                    }
                }
            }
        }
    }

    println!("{}", green!("=== Get Windows bluetooth info from {} ===", red!("{}", device)));

    println!("{} |      {} |      {}", blue!("{:<30}", "Device Name"), blue!("{:<24}", "Address"), blue!("{:<40} ", "Key"));
    println!("{}", "-".repeat(102));
    for (name, info) in &bt_device_info {
        println!("{} |      {} |      {}", rgb!(0xf0, 0x00, 0x56, "{:<30}", name), rgb!(0x00, 0xe0, 0x79, "{:<24}", info.mac), rgb!(0x00, 0xe0, 0x79, "{:<40}", info.ltk));

    }

    Ok(bt_device_info)
}

fn find_and_mount_ntfs_partitions() -> AnyResult<HashMap<String, BtDeviceInfo>> {
    let mut bt_device_info:HashMap<String, BtDeviceInfo> = HashMap::new();

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
                let start = SystemTime::now();
                let since_the_epoch = start.duration_since(UNIX_EPOCH)
                    .expect("Time went backwards");
                let timestamp = since_the_epoch.as_millis();
                let random_suffix: u32 = rand::thread_rng().gen();
            
                let mount_point = format!("/mnt/temp_{}_{}", timestamp, random_suffix);
                fs::create_dir_all(mount_point.clone())?;

                let mount_status = Command::new("mount").args(["-t", "ntfs3", &device, &mount_point]).status()?;
                if mount_status.success() {
                    bt_device_info = parse_reg(&device, &mount_point).context("Failed to parse registry")?;
                    if !bt_device_info.is_empty() {
                        let umount_status = Command::new("umount").arg(mount_point.clone()).status()?;
                        if umount_status.success() {
                            if fs::read_dir(mount_point.clone()).ok().and_then(|mut iter| iter.next()).is_none() {
                                fs::remove_dir(&mount_point)?;
                            } 
                        }
                        break;
                    }
                }
                let umount_status = Command::new("umount").arg(mount_point.clone()).status()?;
                if umount_status.success() {
                    if fs::read_dir(mount_point.clone()).ok().and_then(|mut iter| iter.next()).is_none() {
                        fs::remove_dir(&mount_point)?;
                    } 
                }
            } else {
                let bt_device_info = parse_reg(&name, &mountpoint).context("Failed to parse registry")?;
                if !bt_device_info.is_empty() {
                    break;
                }
            }
        }
    }

    Ok(bt_device_info)
}

pub fn update_bt_info(c: &str, info: &BtDeviceInfo) -> String {
    let mut in_ltk = false;
    let mut updated = String::new();
    for line in c.lines() {
        if line.trim().starts_with("[") {
            in_ltk = false;
        }
        if line.trim() == "[LongTermKey]" {
            in_ltk = true;
        }

        if in_ltk  {
            if line.starts_with("Key=") {
                updated.push_str(&format!("Key={}\n", info.ltk));
                continue;
            } 
            if line.starts_with("EDiv=") {
                updated.push_str(&format!("EDiv={}\n", info.edev));
                continue;
            }
            if line.starts_with("Rand=") {
                updated.push_str(&format!("Rand={}\n", info.erand));
                continue;
            }
        }

        updated.push_str(line);
        updated.push('\n');
        
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
    if std::env::var("TESTING").is_ok() { return;}

    let output = Command::new("systemctl").args(["restart", "bluetooth"]).output().expect("Failed to execute command");

    if output.status.success() {
        println!("\n{}", green!("=== Bluetooth service restarted successfully. ==="));
    } else {
        eprintln!("\nFailed to restart Bluetooth service. Error: {}", String::from_utf8_lossy(&output.stderr));
    }
}

pub fn process_bth_device(path: std::path::PathBuf, bt_device_info: &HashMap<String, BtDeviceInfo>) -> AnyResult<()> {
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
                        if let Some(info) = bt_device_info.get(&name) {
                            let new_content = update_bt_info(&content, &info);
                            fs::write(format!("{}", info_path.to_str().unwrap()), &new_content)?;
                            fs::rename(path, path.parent().unwrap().join(info.mac.clone()))?;
        
                            result_map.insert(name, (file_name.to_string(), info.mac.clone(), ltk_old, info.ltk.clone()));
                        }
                    }
                }
            }

        }
    }

    if result_map.is_empty() {
        println!("{}", green!("\n=== NO Linux bluetooth info found from {} ===", red!("{}", path.to_string_lossy())));
        return Ok(())
    }

    println!("{}", green!("\n=== Update Linux bluetooth info ==="));

    println!("{} |      {} |      {}", blue!("{:<30}", "Device Name"), blue!("{:<24}", "Address"), blue!("{:<40} ", "Key"));
    println!("{}", "-".repeat(102));
    for (name, (old_mac, new_mac, old_ltk, new_ltk)) in &result_map {
        println!("{} | FROM {} | FROM {}", rgb!(0x80, 0x1d, 0xae, "{:<30}", name), rgb!(0xff, 0xa4, 0x00, "{:<24}", old_mac), rgb!(0xff, 0xa4, 0x00, "{:<40}", old_ltk));
        println!("{} |   TO {} |   TO {}", rgb!(0x80, 0x1d, 0xae, "{:<30}", ""),   rgb!(0xaf, 0xdd, 0x22, "{:<24}", new_mac), rgb!(0xaf, 0xdd, 0x22, "{:<40}", new_ltk));
    }

    restart_bluetooth_service();
    Ok(())
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