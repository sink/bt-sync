use bluetooth::*;
use bt_sync::*;
use hive::parse_reg;
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
    let info = BtDeviceInfo {
        mac: "00:00:00:00:00:00".to_string(),
        ltk: "112233445566778899AABBCCDDEEFF".to_string(),
        edev: "12345".to_string(),
        erand: "998877665544".to_string()
    };
    let updated_content = update_bt_info(content, &info);
    assert!(updated_content.contains(&format!("Key={}", info.ltk)));
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

    let mut bt_device_info: HashMap<String, BtDeviceInfo> = HashMap::new();
    let new_ltk = "DEADBEEF00000000DEADBEEF00000000";
    bt_device_info.insert(
        "Basilisk X HyperSpeed".to_string(),
        BtDeviceInfo {
            mac: "00:11:22:33:44:55".to_string(),
            ltk: new_ltk.to_string(),
            edev: "12345".to_string(),
            erand: "998877665544".to_string()
        }
    );

    std::env::set_var("TESTING", "true");
    process_bth_device(temp_dir.path().to_path_buf(), &bt_device_info)?;

    let new_dir = dir.parent().unwrap().join("00:11:22:33:44:55");
    assert!(new_dir.exists());

    let info_path = new_dir.join("info");
    let content = fs::read_to_string(&info_path)?;
    let ltk = get_ltk(&content);
    assert_eq!(new_ltk, ltk);

    Ok(())
}


#[test]
fn test_parse_reg() -> Result<()> {
    let path = Path::new(file!()).parent().unwrap().join("data");
    assert!(path.exists());

    let result = parse_reg("/dev/test", path.to_str().unwrap())?;
    
    let expected_map: HashMap<String, BtDeviceInfo> = [
        ("BT+2.4G KB".to_string(), BtDeviceInfo {
            mac: "E0:10:5F:A9:F6:59".to_string(),
            ltk: "039D9DE0952391208B4F755257E6425B".to_string(),
            edev: "28781".to_string(),
            erand: "16975003643600944841".to_string()
        }),

        ("Basilisk X HyperSpeed".to_string(), BtDeviceInfo {
            mac: "FC:51:CA:AC:57:11".to_string(),
            ltk: "D23FEDC5F5806AF8A37D41D81EE4DA5C".to_string(),
            edev: "34794".to_string(),
            erand: "9659891662176722970".to_string()
        }),
        
        ("Xbox Wireless Controller".to_string(), BtDeviceInfo {
            mac: "AC:8E:BD:24:AC:52".to_string(),
            ltk: "84417A06F13444B2780E0CC3CF1D353D".to_string(),
            edev: "0".to_string(),
            erand: "0".to_string()
        })
    ]
    .iter()
    .cloned()
    .collect();
    assert_eq!(expected_map, result);

    Ok(())
}
