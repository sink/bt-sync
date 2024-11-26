use nt_hive::{Hive, KeyValueData, KeyValueDataType, Result};
use regex::Regex;
use std::collections::HashMap;
use std::fs::File;
use std::io::Read;
use std::io::{self, Write};
use std::path::Path;
use std::process::{Command, Stdio};

fn fmt_mac(mac: &str) -> String {
    if mac.len() % 2 != 0 {
        panic!("MAC address length must be even.");
    }

    (0..mac.len())
        .step_by(2)
        .map(|i| &mac[i..i + 2])
        .map(|s| s.to_uppercase())
        .collect::<Vec<String>>()
        .join(":")
}

fn parse_reg(path: &str) -> Result<HashMap<String, String>, String> {
    let mut file = File::open(path).map_err(|e| format!("Open hive error: {e}"))?;
    let mut buf = Vec::<u8>::new();
    file.read_to_end(&mut buf).map_err(|e| format!("Read hive error: {e}"))?;

    let hive = Hive::new(buf.as_ref()).map_err(|e| format!("Parse hive error: {e}"))?;
    let root = hive.root_key_node().unwrap();
    let keys = root.subpath(r"ControlSet001\Services\BTHPORT\Parameters\Keys").unwrap().unwrap();

    let mut map = HashMap::new();
    if let Some(subkeys) = keys.subkeys() {
        for dev in subkeys.map_err(|e| format!("Get subkeys error: {e}"))? {
            let dev = dev.map_err(|e| format!("Enumerate key error: {e}"))?;
            if let Some(subs) = dev.subkeys() {
                for key in subs.map_err(|e| format!("Get subkeys error: {e}"))? {
                    let key = key.map_err(|e| format!("Enumerate key error: {e}"))?;
                    let name = key.name().map_err(|e| format!("Get name error: {e}"))?;
                    println!("BT name: {}", name);
                    if let Some(val) = key.value("LTK") {
                        let val = val.unwrap();
                        let vtype = val.data_type().map_err(|e| format!("Get type error: {e}"))?;
                        if vtype != KeyValueDataType::RegBinary { continue; }
                        match val.data().map_err(|e| format!("Get binary data error: {e}"))? {
                            KeyValueData::Small(data) => {
                                let ltk = data.iter().map(|b| format!("{:02X}", b)).collect::<String>();
                                let mac = fmt_mac(&name.to_string());
                                map.insert(mac, ltk);
                            },
                            KeyValueData::Big(_) => println!("BIG DATA"),
                        }
                    }
                }
            }
        }
    }
    Ok(map)
}

fn get_bt_dir() -> Option<String> {
    let output = Command::new("sudo").arg("ls").arg("/var/lib/bluetooth/").output().ok()?;
    if !output.status.success() {
        eprintln!("List dir error: {}", String::from_utf8_lossy(&output.stderr));
        return None;
    }
    let binding = String::from_utf8_lossy(&output.stdout);
    let dirs: Vec<&str> = binding.trim().split('\n').collect();
    if dirs.is_empty() { eprintln!("No BT dir found"); None } else { Some(dirs[0].to_string()) }
}

fn read_file(path: &str) -> io::Result<String> {
    let output = Command::new("sudo").arg("cat").arg(path).output()?;
    if !output.status.success() {
        eprintln!("Read file error: {}", String::from_utf8_lossy(&output.stderr));
        Err(io::Error::new(io::ErrorKind::Other, "Failed to read"))
    } else {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    }
}

fn write_file(path: &str, content: &str) -> io::Result<()> {
    let mut child = Command::new("sudo").arg("tee").arg(path).stdin(Stdio::piped()).stdout(Stdio::null()).spawn()?;
    child.stdin.as_mut().unwrap().write_all(content.as_bytes())?;
    child.wait()?;
    Ok(())
}

fn update_ltk(c: &str, ltk: &str) -> String {
    let mut in_ltk = false;
    let ends_nl = c.ends_with('\n');
    let updated = c.lines().map(|l| {
        if l.trim() == "[LongTermKey]" { in_ltk = true; l.to_string() }
        else if in_ltk && l.starts_with("Key=") { format!("Key={}", ltk) }
        else { l.to_string() }
    }).collect::<Vec<_>>().join("\n");
    if ends_nl && !updated.ends_with('\n') { updated + "\n" } else { updated }
}

fn proc_dev(path: &str, map: &HashMap<String, String>) -> io::Result<()> {
    let output = Command::new("sudo").arg("ls").arg(path).output()?;
    if !output.status.success() {
        eprintln!("List dir error: {}", String::from_utf8_lossy(&output.stderr));
        return Ok(());
    }
    let binding = String::from_utf8_lossy(&output.stdout);
    let devices: Vec<&str> = binding.trim().split('\n').collect();
    let name_re = Regex::new(r"(?m)^Name=(.*)$").unwrap();
    for dev in devices {
        if !dev.contains(':') { continue; }
        let info_path = format!("{}/{}/info", path, dev);
        let content = read_file(&info_path)?;
        if let Some(caps) = name_re.captures(&content) {
            let name = caps.get(1).map_or("", |m| m.as_str());
            println!("Proc device: {}, Name: {}", dev, name);
            for (mac, ltk) in map {
                if dev.starts_with(&mac[..8]) {
                    println!("  Update LTK: {}", ltk);
                    let new_content = update_ltk(&content, ltk);
                    let updated_path = format!("{}.updated", info_path);
                    write_file(&updated_path, &new_content)?;
                    let new_name = mac.clone();
                    let new_path = format!("{}/{}", path, new_name);
                    if new_path != format!("{}/{}", path, dev) {
                        Command::new("sudo")
                            .arg("mv")
                            .arg(format!("{}/{}", path, dev))
                            .arg(new_path)
                            .output()?;
                        println!("  Renamed from {} to {}", dev, new_name);
                    }
                    break;
                }
            }
        }
    }
    Ok(())
}

fn list_ntfs_mounts() -> Vec<(String, String)> {
    let output = Command::new("mount").output().expect("Mount cmd failed");
    let binding = String::from_utf8_lossy(&output.stdout);
    let lines = binding.lines();
    lines.filter_map(|line| {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() > 4 && ["ntfs", "ntfs3"].contains(&parts[4].to_lowercase().as_str()) {
            Some((parts[0].to_string(), parts[2].to_string()))
        } else { None }
    }).collect()
}

fn main() {
    let mounts = list_ntfs_mounts();
    if mounts.is_empty() { eprintln!("No NTFS mounts found."); return; }

    let mut ltk_map = HashMap::new();
    for (_device, mount) in mounts {
        let reg_path = format!("{}/Windows/System32/config/SYSTEM", mount);
        if Path::new(&reg_path).exists() {
            match parse_reg(&reg_path) {
                Ok(parsed) => {
                    if !parsed.is_empty() {
                        println!("Parsed LTK:");
                        for (mac, ltk) in &parsed { println!("{} = {}", mac, ltk); }
                        ltk_map = parsed; 
                        break;
                    } else { eprintln!("No LTK to show."); }
                },
                Err(e) => eprintln!("Parse reg error: {}", e),
            }
        }
    }

    if let Some(bt_dir) = get_bt_dir() {
        let bt_path = format!("/var/lib/bluetooth/{}", bt_dir);
        if let Err(e) = proc_dev(&bt_path, &ltk_map) { eprintln!("Process device error: {}", e); }
    }
}