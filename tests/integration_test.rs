use bt_sync::*; // 导入 bt-sync 项目中的所有公共项
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use anyhow::Result;

// 测试 fmt_mac 函数
#[test]
fn test_fmt_mac() {
    assert_eq!(fmt_mac("001122334455"), "00:11:22:33:44:55");
}

// 测试 update_ltk 函数
#[test]
fn test_update_ltk() -> Result<()> {
    let content = r#"
[LongTermKey]
Key=00000000000000000000000000000000
"#;
    let ltk = "112233445566778899AABBCCDDEEFF";
    let updated_content = update_ltk(content, ltk);
    assert!(updated_content.contains(&format!("Key={}", ltk)));
    Ok(())
}

// 测试 process_bth_device 函数
#[test]
fn test_process_bth_device() -> Result<()> {
    let dir = tempdir()?.path().join("00:00:00:00:00:00");
    fs::create_dir_all(&dir)?;
    let info_path =dir.join("info");
    fs::write(&info_path, "name=TestDevice\n")?;

    println!("PP={}", info_path.to_string_lossy());
    // 创建 HashMap
    let mut bt_device_info: HashMap<String, (String, String)> = HashMap::new();
    bt_device_info.insert(
        "TestDevice".to_string(),
        ("00:11:22:33:44:55".to_string(), "AA:BB:CC:DD:EE:FF:00:11:22:33:44:55:66:77:88:99".to_string()),
    );

    process_bth_device(dir.to_str().unwrap(), &bt_device_info)?;

    let new_dir = dir.parent().unwrap().join("00:11:22:33:44:55");
    assert!(new_dir.exists());

    let info_path_updated = new_dir.join("info.updated");
    assert!(info_path_updated.exists());

    Ok(())
}

// 测试 parse_reg 函数
#[test]
fn test_parse_reg() -> Result<()> {
    let path = Path::new(file!()).parent().unwrap().join("data/SYSTEM");
    assert!(path.exists());

    let result = parse_reg(path.to_str().unwrap())?;
    
    let expected_map: HashMap<String, (String, String)> = [
        ("BT+2.4G KB".to_string(), ("E0:10:5F:A9:F6:59".to_string(), "039D9DE0952391208B4F755257E6425B".to_string())),
        ("Basilisk X HyperSpeed".to_string(), ("FC:51:CA:AC:57:11".to_string(), "D23FEDC5F5806AF8A37D41D81EE4DA5C".to_string())),
        ("Xbox Wireless Controller".to_string(), ("AC:8E:BD:24:AC:52".to_string(), "84417A06F13444B2780E0CC3CF1D353D".to_string()))
    ]
    .iter()
    .cloned()
    .collect();
    assert_eq!(result, expected_map);

    Ok(())
}
