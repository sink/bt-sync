use bt_sync::*;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use tempfile::tempdir;
use anyhow::Result;

#[test]
fn test_fmt_mac() {
    assert_eq!(fmt_mac("001122334455"), "00:11:22:33:44:55");
}

#[test]
fn test_update_ltk() -> Result<()> {
    let content = r#"
[LongTermKey]
Key=00000000000000000000000000000000
Name=test"#;
    let ltk = "112233445566778899AABBCCDDEEFF";
    let updated_content = update_ltk(content, ltk);
    assert!(updated_content.contains(&format!("Key={}", ltk)));
    Ok(())
}

#[test]
fn test_process_bth_device() -> Result<(), Box<dyn std::error::Error>> {
    let temp_dir = tempdir()?;
    let dir = temp_dir.path().join("00:00:00:00:00:00");
    fs::create_dir_all(&dir)?;

    let info_content = r#"[General]
Name=Basilisk X HyperSpeed
Appearance=0x03c2
AddressType=static
SupportedTechnologies=LE;
Trusted=true
Blocked=false
WakeAllowed=true
Services=00001800

[IdentityResolvingKey]
Key=8EC94951919F694C8DBFD5E0BEA21536

[LongTermKey]
Key=D23FEDC5F5806AF8A37D41D81EE4DA5C
Authenticated=0
EncSize=16
EDiv=17209
Rand=189227263063048024

[PeripheralLongTermKey]
Key=414C87970DBAE282734D2BDCC1157C30
Authenticated=0
EncSize=16
EDiv=27023
Rand=15138338010761522440

[SlaveLongTermKey]
Key=414C87970DBAE282734D2BDCC1157C30
Authenticated=0
EncSize=16
EDiv=27023
Rand=15138338010761522440

[ConnectionParameters]
MinInterval=6
MaxInterval=9
Latency=20
Timeout=300

[DeviceID]
Source=2
Vendor=5426
Product=130
Version=1"#;

    let info_path = dir.join("info");
    fs::write(&info_path, info_content)?;

    println!("PP={}", info_path.to_string_lossy());

    let mut bt_device_info: HashMap<String, (String, String)> = HashMap::new();
    let new_ltk = "DEADBEEF00000000DEADBEEF00000000";
    bt_device_info.insert(
        "Basilisk X HyperSpeed".to_string(),
        ("00:11:22:33:44:55".to_string(), new_ltk.to_string()),
    );

    std::env::set_var("TESTING", "true");
    process_bth_device(temp_dir.path().to_path_buf(), &bt_device_info)?;

    let new_dir = dir.parent().unwrap().join("00:11:22:33:44:55");
    assert!(new_dir.exists());

    let info_path = new_dir.join("info");
    let content = fs::read_to_string(&info_path)?;
    let ltk = get_ltk(&content);
    assert_eq!(ltk, new_ltk);

    Ok(())
}


#[test]
fn test_parse_reg() -> Result<()> {
    let path = Path::new(file!()).parent().unwrap().join("data");
    assert!(path.exists());

    let result = parse_reg("/dev/test", path.to_str().unwrap())?;
    
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
