// tests/integration_test.rs

use bt_sync::{fmt_mac, parse_reg, get_bt_dir, update_ltk, proc_dev, list_ntfs_mounts};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use mockall::predicate::*;
use mockall::*;
use std::process::{Command, Output};
use std::os::unix::process::ExitStatusExt;  // 添加这个导入

#[test]
fn test_fmt_mac() {
    assert_eq!(fmt_mac("1234567890ab"), "12:34:56:78:90:AB");
}

#[test]
fn test_parse_reg() {
    let current_dir = env!("CARGO_MANIFEST_DIR");
    let path = Path::new(current_dir).join("data/SYSTEM");

    match parse_reg(path.to_str().unwrap()) {
        Ok(map) => {
            assert!(!map.is_empty());
        },
        Err(e) => {
            eprintln!("Failed to parse reg: {}", e);
            assert!(false); // 如果失败，测试应该失败
        }
    }
}

#[test]
fn test_get_bt_dir() {
    let bt_dir = get_bt_dir();

    assert_eq!(bt_dir, Some("12:34:56:78:90:AB".to_string()));
}

#[test]
fn test_update_ltk() {
    let original_content = "[LongTermKey]\nKey=OldLTK\n";
    let updated_content = update_ltk(original_content, "NewLTK");
    assert_eq!(updated_content, "[LongTermKey]\nKey=NewLTK\n");
}

#[test]
fn test_proc_dev() {
    let test_path = "/tmp/bt-test";
    fs::create_dir_all(test_path).unwrap();

    // 创建模拟的蓝牙设备文件
    let device_info = "Name=TestDevice\n";
    let device_path = format!("{}/12345678/info", test_path);
    fs::write(&device_path, device_info).unwrap();

    // 创建模拟的 LTK 映射
    let mut ltk_map = HashMap::new();
    ltk_map.insert("12:34:56:78:90:AB".to_string(), "NewLTK".to_string());

    // 执行 proc_dev
    proc_dev(test_path, &ltk_map).unwrap();

    // 检查文件是否被更新
    let updated_path = format!("{}/12345678/info.updated", test_path);
    let updated_content = fs::read_to_string(updated_path).unwrap();
    assert_eq!(updated_content, "[LongTermKey]\nKey=NewLTK\n");

    // 清理
    fs::remove_dir_all(test_path).unwrap();
}

#[test]
fn test_list_ntfs_mounts() {
    let mounts = list_ntfs_mounts();
    for (device, mount_point) in mounts {
        assert!(Path::new(&mount_point).exists());
    }
}