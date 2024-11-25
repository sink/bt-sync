use nt_hive::{Hive, KeyNode, KeyValueData, KeyValueDataType, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::io::{self, Write};
use std::io::{Error, ErrorKind};
use std::path::Path;
use std::process::{Command, Stdio};

fn format_mac_address(mac: &str) -> String {
    mac.as_bytes()
        .chunks(2)
        .map(|chunk| format!("{:02X}", chunk[0]))
        .collect::<Vec<String>>()
        .join(":")
}

fn parse_registry(reg_path: &str) -> Result<HashMap<String, String>, String> {
    // Read the hive file.
    let filename = reg_path;
    let mut f = File::open(filename).map_err(|e| format!("Error opening hive file: {e}"))?;
    let mut buffer = Vec::<u8>::new();
    f.read_to_end(&mut buffer)
        .map_err(|e| format!("Error reading hive file: {e}"))?;

    // Parse the hive.
    let hive = Hive::new(buffer.as_ref()).map_err(|e| format!("Error parsing hive file: {e}"))?;
    let root_key_node = hive.root_key_node().unwrap();
    let key_node = root_key_node
        .subpath(r"ControlSet001\Services\BTHPORT\Parameters\Keys")
        .unwrap()
        .unwrap();

    let mut result: HashMap<String, String> = HashMap::new(); // Explicitly initialize the result map with types

    if let Some(subkeys) = key_node.subkeys() {
        let subkeys = subkeys.map_err(|e| format!("Error getting subkeys: {e}"))?;

        for bth_device_node in subkeys {
            let bth_device_node =
                bth_device_node.map_err(|e| format!("Error enumerating key: {e}"))?;

            if let Some(subkeys) = bth_device_node.subkeys() {
                let subkeys = subkeys.map_err(|e| format!("Error getting subkeys: {e}"))?;

                for key_node in subkeys {
                    let key_node = key_node.map_err(|e| format!("Error enumerating key: {e}"))?;
                    let key_name = key_node
                        .name()
                        .map_err(|e| format!("Error getting key name: {e}"))?;

                    // Print the names of the values of this node.
                    if let Some(value_iter) = key_node.values() {
                        let value_iter = value_iter
                            .map_err(|e| format!("Error creating value iterator: {e}"))?;

                        for value in value_iter {
                            let value =
                                value.map_err(|e| format!("Error enumerating value: {e}"))?;

                            let value_name = value
                                .name()
                                .map_err(|e| format!("Error getting value name: {e}"))?
                                .to_string_lossy();

                            let value_type = value
                                .data_type()
                                .map_err(|e| format!("Error getting value type: {e}"))?;

                            if value_name != "LTK" || value_type != KeyValueDataType::RegBinary {
                                continue;
                            }

                            let binary_data = value
                                .data()
                                .map_err(|e| format!("Error getting binary data: {e}"))?;
                            match binary_data {
                                KeyValueData::Small(data) => {
                                    // Convert binary data to hexadecimal string
                                    let ltk_hex = data
                                        .iter()
                                        .map(|b| format!("{:02X}", b))
                                        .collect::<Vec<_>>()
                                        .join("");
                                    // Insert the LTK into the result map

                                    let formatted_mac = format_mac_address(&key_name.to_string());
                                    result.insert(formatted_mac, ltk_hex); // Use clone() because btu_device_name is used multiple times
                                }
                                KeyValueData::Big(_iter) => println!("BIG DATA"),
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(result)
}

fn get_device_directory_with_sudo() -> Option<String> {
    let output = Command::new("sudo")
        .arg("ls")
        .arg("/var/lib/bluetooth/")
        .output()
        .ok()?;
    if !output.status.success() {
        eprintln!(
            "Error listing device directory: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return None;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let dirs: Vec<&str> = stdout.trim().split('\n').collect();
    if dirs.is_empty() {
        eprintln!("No device directory found");
        return None;
    }
    Some(dirs[0].to_string())
}

fn read_file_with_sudo(file_path: &str) -> io::Result<String> {
    let output = Command::new("sudo").arg("cat").arg(file_path).output()?;
    if !output.status.success() {
        eprintln!(
            "Error reading file: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(io::Error::new(io::ErrorKind::Other, "Failed to read file"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

fn write_file_with_sudo(file_path: &str, content: &str) -> io::Result<()> {
    let mut child = Command::new("sudo")
        .arg("tee")
        .arg(file_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .spawn()?;
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(content.as_bytes())?;
    child.wait()?;
    Ok(())
}

fn process_device(device_path: &str, ltk_map: &HashMap<String, String>) -> io::Result<()> {
    let output = Command::new("sudo")
        .arg("ls")
        .arg(device_path)
        .output()?;

    if !output.status.success() {
        eprintln!(
            "Error listing device directory: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let stdout = String::from_utf8_lossy(&output.stdout);    
    let devices: Vec<&str> = stdout.trim().split('\n').collect();

    let name_re = Regex::new(r"^Name=(.*)$").unwrap();
    let key_re = Regex::new(r"^Key=.*$").unwrap();

    for device in devices {
        if !device.contains(':') {
            continue;
        }

        println!(" device:{}", device); // 打印 stdout

        let info_file = format!("{}/{}/info", device_path, device);
        let content = read_file_with_sudo(&info_file)?;

        // println!(" content:{}", content); // 打印 stdout

        if let Some(caps) = name_re.captures(&content) {


            let name = caps.get(1).map_or("", |m| m.as_str());

            println!("Processing device: {}", device);
            println!("  Device Name: {}", name);

            for (mac, ltk) in ltk_map {
                if device.starts_with(&mac[..8]) {
                    let updated_content = key_re.replace(&content, format!("Key={}", ltk));
                    // write_file_with_sudo(&info_file, &updated_content)?;

                    let new_device_name = mac.clone();
                    let new_device_path = format!("{}/{}", device_path, new_device_name);
                    if new_device_path != format!("{}/{}", device_path, device) {
                        // Command::new("sudo")
                        //     .arg("mv")
                        //     .arg(format!("{}/{}", device_path, device))
                        //     .arg(new_device_path)
                        //     .output()?;
                        println!("  Renamed directory from {} to {}", device, new_device_name);
                    }

                    break;
                }
            }
        }
    }
    Ok(())
}

fn list_ntfs_mount_points() -> Vec<(String, String)> {
    let output = Command::new("mount")
        .output()
        .expect("Failed to execute mount command");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .lines()
        .filter_map(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() > 4 && ["ntfs", "ntfs3"].contains(&parts[4].to_lowercase().as_str()) {
                Some((parts[0].to_string(), parts[2].to_string()))
            } else {
                None
            }
        })
        .collect()
}

fn main() {
    let ntfs_mounts = list_ntfs_mount_points();
    if ntfs_mounts.is_empty() {
        eprintln!("No NTFS mount points found.");
        return;
    }

    let mut content: HashMap<String, String> = HashMap::new(); // 初始化 content

    for (device, mount_point) in ntfs_mounts {
        let reg_path = format!("{}/Windows/System32/config/SYSTEM", mount_point);
        if Path::new(&reg_path).exists() {
            match parse_registry(&reg_path) {
                Ok(parsed_content) => {
                    if !parsed_content.is_empty() {
                        println!("Parsed content:");
                        for (mac, ltk) in &parsed_content {
                            println!("{} = {}", mac, ltk);
                        }
                        content = parsed_content; // 将解析的内容赋值给 content
                        break; // Only process the first found SYSTEM file
                    } else {
                        eprintln!("No content to display.");
                    }
                }
                Err(e) => {
                    eprintln!("Error parsing registry: {}", e);
                }
            }
        }
    }

    if let Some(device_dir) = get_device_directory_with_sudo() {
        let device_path = format!("/var/lib/bluetooth/{}", device_dir);
        if let Err(e) = process_device(&device_path, &content) {
            eprintln!("Failed to process device: {}", e);
        }
    }
}
