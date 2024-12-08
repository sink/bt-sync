use std::{collections::HashMap, fs, process::Command, time::{SystemTime, UNIX_EPOCH}};
use anyhow::Result;
use rand::Rng;
use regex::Regex;

use crate::{bluetooth::BtDeviceInfo, hive::parse_reg};

pub fn find_and_mount_ntfs_partitions() -> Result<HashMap<String, BtDeviceInfo>> {
    let mut bt_device_info: HashMap<String, BtDeviceInfo> = HashMap::new();
    let partitions = get_partitions_info()?;
    
    for partition in partitions {
        if partition.fstype == "ntfs" {
            if partition.mountpoint.is_empty() {
                if let Some(info) = mount_and_parse_partition(&partition.device)? {
                    bt_device_info = info;
                    break;
                }
            } else {
                let info = parse_reg(&partition.device, &partition.mountpoint)?;
                if !info.is_empty() {
                    bt_device_info = info;
                    break;
                }
            }
        }
    }

    Ok(bt_device_info)
}

#[derive(Debug)]
struct PartitionInfo {
    fstype: String,
    mountpoint: String,
    device: String,
}

fn get_partitions_info() -> Result<Vec<PartitionInfo>> {
    let output = Command::new("lsblk")
        .args(["-o", "NAME,FSTYPE,MOUNTPOINT", "--pairs", "--noheadings"])
        .output()?;
    
    if !output.status.success() {
        return Ok(vec![]);
    }

    let re = Regex::new(r#"NAME="([^"]+)" FSTYPE="([^"]+)" MOUNTPOINT="([^"]*)""#).unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let partitions = re.captures_iter(&stdout)
        .map(|cap| PartitionInfo {
            fstype: cap[2].to_string(),
            mountpoint: cap[3].to_string(),
            device: format!("/dev/{}", &cap[1]),
        })
        .collect();

    Ok(partitions)
}

fn mount_and_parse_partition(device: &str) -> Result<Option<HashMap<String, BtDeviceInfo>>> {
    let mount_point = create_temp_mount_point()?;
    match mount_partition(device, &mount_point) {
        Ok(_) => {
            let info = parse_reg(device, &mount_point)?;
            umount_and_cleanup(&mount_point)?;
            Ok(Some(info))
        },
        Err(e) => {
            umount_and_cleanup(&mount_point)?;
            println!("Failed to mount {}: {}", device, e);
            Ok(None)
        }
    }
}

fn create_temp_mount_point() -> Result<String> {
    let start = SystemTime::now();
    let since_the_epoch = start.duration_since(UNIX_EPOCH)
        .expect("Time went backwards");
    let timestamp = since_the_epoch.as_millis();
    let random_suffix: u32 = rand::thread_rng().gen();
    let mount_point = format!("/mnt/temp_{}_{}", timestamp, random_suffix);
    fs::create_dir_all(&mount_point)?;
    Ok(mount_point)
}

fn mount_partition(device: &str, mount_point: &str) -> Result<()> {
    let status = Command::new("mount").args(["-t", "ntfs3", device, mount_point]).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Failed to mount {}", device))
    }
}

fn umount_and_cleanup(mount_point: &str) -> Result<()> {
    let status = Command::new("umount").arg(mount_point).status()?;
    if status.success() && is_directory_empty(mount_point)? {
        fs::remove_dir(mount_point)?;
    }
    Ok(())
}

fn is_directory_empty(path: &str) -> Result<bool> {
    Ok(fs::read_dir(path)?.next().is_none())
}