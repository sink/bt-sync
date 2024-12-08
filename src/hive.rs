use std::{collections::HashMap, fs, io::Read};

use anyhow::{Context, Result};
use nt_hive::{Hive, KeyValueData};
use term_ansi::{blue, green, red, rgb};

use crate::bluetooth::{fmt_mac, BtDeviceInfo};


pub fn parse_reg(device: &str, mountpoint: &str) -> Result<HashMap<String, BtDeviceInfo>> {
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