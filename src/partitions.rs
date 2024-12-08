use std::{collections::HashMap, fs, process::Command, time::{SystemTime, UNIX_EPOCH}};

use anyhow::Result;
use rand::Rng;
use regex::Regex;

use crate::{bluetooth::BtDeviceInfo, hive::parse_reg};


pub fn find_and_mount_ntfs_partitions() -> Result<HashMap<String, BtDeviceInfo>> {
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
                    bt_device_info = parse_reg(&device, &mount_point)?;
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
                let bt_device_info = parse_reg(&name, &mountpoint)?;
                if !bt_device_info.is_empty() {
                    break;
                }
            }
        }
    }

    Ok(bt_device_info)
}
