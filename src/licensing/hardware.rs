use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

pub const HARDWARE_SCHEMA_VERSION: &str = "hw-v1";

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareComponentKind {
    SystemUuid,
    SystemSerial,
    BaseboardSerial,
    ChassisSerial,
    SystemVolume,
    MachineGuid,
}

impl HardwareComponentKind {
    fn domain(self) -> &'static str {
        match self {
            Self::SystemUuid => "system-uuid",
            Self::SystemSerial => "system-serial",
            Self::BaseboardSerial => "baseboard-serial",
            Self::ChassisSerial => "chassis-serial",
            Self::SystemVolume => "system-volume",
            Self::MachineGuid => "machine-guid",
        }
    }

    pub fn as_str(self) -> &'static str {
        self.domain()
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HardwareComponent {
    pub kind: HardwareComponentKind,
    pub digest: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HardwareIdentityStrength {
    Weak,
    Standard,
    Strong,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct HardwareIdentity {
    pub schema: String,
    pub fingerprint: String,
    pub components: Vec<HardwareComponent>,
    pub strength: HardwareIdentityStrength,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HardwareIdentityError {
    UnsupportedPlatform,
    FirmwareUnavailable,
    NoStableIdentifier,
    WindowsApi(u32),
}

impl fmt::Display for HardwareIdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => f.write_str("hardware identity is supported on Windows"),
            Self::FirmwareUnavailable => f.write_str("SMBIOS firmware data is unavailable"),
            Self::NoStableIdentifier => f.write_str("no stable hardware identifier is available"),
            Self::WindowsApi(code) => write!(f, "Windows API error {code}"),
        }
    }
}

impl std::error::Error for HardwareIdentityError {}

#[derive(Default)]
struct RawHardware {
    system_uuid: Option<String>,
    system_serial: Option<String>,
    baseboard_serial: Option<String>,
    chassis_serial: Option<String>,
    system_volume: Option<String>,
    machine_guid: Option<String>,
}

impl HardwareIdentity {
    fn from_raw(raw: RawHardware) -> Result<Self, HardwareIdentityError> {
        let normalized = [
            (HardwareComponentKind::SystemUuid, raw.system_uuid),
            (HardwareComponentKind::SystemSerial, raw.system_serial),
            (HardwareComponentKind::BaseboardSerial, raw.baseboard_serial),
            (HardwareComponentKind::ChassisSerial, raw.chassis_serial),
            (HardwareComponentKind::SystemVolume, raw.system_volume),
            (HardwareComponentKind::MachineGuid, raw.machine_guid),
        ]
        .into_iter()
        .filter_map(|(kind, value)| {
            value
                .as_deref()
                .and_then(normalize_identifier)
                .map(|value| (kind, value))
        })
        .collect::<Vec<_>>();

        let primary = normalized
            .iter()
            .find(|(kind, _)| *kind == HardwareComponentKind::SystemUuid)
            .map(|(_, value)| value.clone())
            .or_else(|| {
                let system = normalized
                    .iter()
                    .find(|(kind, _)| *kind == HardwareComponentKind::SystemSerial)?;
                let board = normalized
                    .iter()
                    .find(|(kind, _)| *kind == HardwareComponentKind::BaseboardSerial)?;
                Some(format!("{}|{}", system.1, board.1))
            })
            .or_else(|| {
                normalized
                    .iter()
                    .find(|(kind, _)| *kind == HardwareComponentKind::MachineGuid)
                    .map(|(_, value)| value.clone())
            })
            .ok_or(HardwareIdentityError::NoStableIdentifier)?;

        let components = normalized
            .iter()
            .map(|(kind, value)| HardwareComponent {
                kind: *kind,
                digest: domain_hash(&format!("component/{}", kind.domain()), value),
            })
            .collect::<Vec<_>>();

        let strength = match components.len() {
            0..=2 => HardwareIdentityStrength::Weak,
            3 => HardwareIdentityStrength::Standard,
            _ => HardwareIdentityStrength::Strong,
        };

        Ok(Self {
            schema: HARDWARE_SCHEMA_VERSION.to_owned(),
            fingerprint: domain_hash("machine", &primary),
            components,
            strength,
        })
    }
}

fn domain_hash(domain: &str, value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"heat3/povorotnik/");
    hasher.update(HARDWARE_SCHEMA_VERSION.as_bytes());
    hasher.update(b"/");
    hasher.update(domain.as_bytes());
    hasher.update([0]);
    hasher.update(value.as_bytes());
    hex_lower(&hasher.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn normalize_identifier(value: &str) -> Option<String> {
    let normalized = value
        .trim_matches(|character: char| character.is_whitespace() || character == '\0')
        .to_ascii_uppercase();
    let compact = normalized
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .collect::<String>();

    if compact.len() < 6
        || compact.chars().all(|character| character == '0')
        || compact.chars().all(|character| character == 'F')
    {
        return None;
    }

    const PLACEHOLDERS: &[&str] = &[
        "TOBEFILLEDBYOEM",
        "DEFAULTSTRING",
        "SYSTEMSERIALNUMBER",
        "BASEBOARDSERIALNUMBER",
        "CHASSISSERIALNUMBER",
        "NOTAPPLICABLE",
        "UNKNOWN",
        "NONE",
    ];
    if PLACEHOLDERS
        .iter()
        .any(|placeholder| compact == *placeholder)
    {
        return None;
    }

    Some(normalized)
}

#[derive(Default)]
struct SmbiosIdentity {
    system_uuid: Option<String>,
    system_serial: Option<String>,
    baseboard_serial: Option<String>,
    chassis_serial: Option<String>,
}

fn parse_smbios_table(table: &[u8]) -> SmbiosIdentity {
    let mut result = SmbiosIdentity::default();
    let mut cursor = 0usize;

    while cursor + 4 <= table.len() {
        let structure_type = table[cursor];
        let formatted_len = table[cursor + 1] as usize;
        if formatted_len < 4 || cursor + formatted_len > table.len() {
            break;
        }

        let formatted = &table[cursor..cursor + formatted_len];
        let strings_start = cursor + formatted_len;
        let Some((strings, next_cursor)) = parse_smbios_strings(table, strings_start) else {
            break;
        };

        match structure_type {
            1 => {
                result.system_serial = string_at(&strings, formatted.get(7).copied());
                if formatted.len() >= 24 {
                    let uuid = &formatted[8..24];
                    if !uuid.iter().all(|byte| *byte == 0) && !uuid.iter().all(|byte| *byte == 0xff)
                    {
                        result.system_uuid = Some(hex_lower(uuid));
                    }
                }
            }
            2 => result.baseboard_serial = string_at(&strings, formatted.get(7).copied()),
            3 => result.chassis_serial = string_at(&strings, formatted.get(7).copied()),
            127 => break,
            _ => {}
        }

        cursor = next_cursor;
    }

    result
}

fn parse_smbios_strings(table: &[u8], start: usize) -> Option<(Vec<String>, usize)> {
    if start >= table.len() {
        return None;
    }
    if start + 1 < table.len() && table[start] == 0 && table[start + 1] == 0 {
        return Some((Vec::new(), start + 2));
    }

    let mut strings = Vec::new();
    let mut cursor = start;
    loop {
        let end = table[cursor..].iter().position(|byte| *byte == 0)? + cursor;
        strings.push(String::from_utf8_lossy(&table[cursor..end]).into_owned());
        cursor = end + 1;
        if cursor >= table.len() {
            return None;
        }
        if table[cursor] == 0 {
            return Some((strings, cursor + 1));
        }
    }
}

fn string_at(strings: &[String], index: Option<u8>) -> Option<String> {
    let index = usize::from(index?);
    if index == 0 {
        return None;
    }
    strings.get(index - 1).cloned()
}

#[cfg(windows)]
pub fn collect_hardware_identity() -> Result<HardwareIdentity, HardwareIdentityError> {
    let smbios = read_smbios_identity().ok();
    HardwareIdentity::from_raw(RawHardware {
        system_uuid: smbios
            .as_ref()
            .and_then(|identity| identity.system_uuid.clone()),
        system_serial: smbios
            .as_ref()
            .and_then(|identity| identity.system_serial.clone()),
        baseboard_serial: smbios
            .as_ref()
            .and_then(|identity| identity.baseboard_serial.clone()),
        chassis_serial: smbios
            .as_ref()
            .and_then(|identity| identity.chassis_serial.clone()),
        system_volume: read_system_volume_serial().ok(),
        machine_guid: read_machine_guid().ok(),
    })
}

/// Conservative offline comparison that tolerates a limited component change
/// without turning a copied DPAPI record into a portable license.
pub fn matches_stored_hardware(
    stored_schema: &str,
    stored_fingerprint: &str,
    stored: &[HardwareComponent],
    current: &HardwareIdentity,
) -> bool {
    if stored_schema != current.schema || stored.is_empty() || current.components.is_empty() {
        return false;
    }
    let matches = stored
        .iter()
        .filter(|expected| current.components.iter().any(|actual| actual == *expected))
        .count();

    if stored_fingerprint == current.fingerprint {
        matches >= stored.len().div_ceil(2).max(1)
    } else {
        stored.len() >= 4 && matches >= 3 && matches * 3 >= stored.len() * 2
    }
}

#[cfg(not(windows))]
pub fn collect_hardware_identity() -> Result<HardwareIdentity, HardwareIdentityError> {
    Err(HardwareIdentityError::UnsupportedPlatform)
}

#[cfg(windows)]
fn read_smbios_identity() -> Result<SmbiosIdentity, HardwareIdentityError> {
    use std::ptr::null_mut;
    use windows_sys::Win32::System::SystemInformation::GetSystemFirmwareTable;

    const RSMB: u32 = u32::from_le_bytes(*b"RSMB");
    let required = unsafe { GetSystemFirmwareTable(RSMB, 0, null_mut(), 0) };
    if required < 8 {
        return Err(HardwareIdentityError::FirmwareUnavailable);
    }

    let mut buffer = vec![0u8; required as usize];
    let received =
        unsafe { GetSystemFirmwareTable(RSMB, 0, buffer.as_mut_ptr(), buffer.len() as u32) };
    if received < 8 || received as usize > buffer.len() {
        return Err(HardwareIdentityError::FirmwareUnavailable);
    }
    buffer.truncate(received as usize);

    let table_len = u32::from_le_bytes(buffer[4..8].try_into().unwrap()) as usize;
    let end = 8usize
        .checked_add(table_len)
        .filter(|end| *end <= buffer.len())
        .ok_or(HardwareIdentityError::FirmwareUnavailable)?;
    Ok(parse_smbios_table(&buffer[8..end]))
}

#[cfg(windows)]
fn read_machine_guid() -> Result<String, HardwareIdentityError> {
    use std::ffi::c_void;
    use std::ptr::null_mut;
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};

    let subkey = wide("SOFTWARE\\Microsoft\\Cryptography");
    let name = wide("MachineGuid");
    let mut bytes = 0u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            null_mut(),
            &mut bytes,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(HardwareIdentityError::WindowsApi(status));
    }

    let mut buffer = vec![0u16; (bytes as usize).div_ceil(2)];
    let status = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buffer.as_mut_ptr().cast::<c_void>(),
            &mut bytes,
        )
    };
    if status != ERROR_SUCCESS {
        return Err(HardwareIdentityError::WindowsApi(status));
    }
    let length = buffer
        .iter()
        .position(|value| *value == 0)
        .unwrap_or(buffer.len());
    let value = String::from_utf16_lossy(&buffer[..length]);
    normalize_identifier(&value).ok_or(HardwareIdentityError::NoStableIdentifier)
}

#[cfg(windows)]
fn read_system_volume_serial() -> Result<String, HardwareIdentityError> {
    use std::ptr::null_mut;
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;

    let drive = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".to_owned());
    let root = wide(&format!("{}\\", drive.trim_end_matches('\\')));
    let mut serial = 0u32;
    let ok = unsafe {
        GetVolumeInformationW(
            root.as_ptr(),
            null_mut(),
            0,
            &mut serial,
            null_mut(),
            null_mut(),
            null_mut(),
            0,
        )
    };
    if ok == 0 {
        return Err(HardwareIdentityError::WindowsApi(
            std::io::Error::last_os_error().raw_os_error().unwrap_or(0) as u32,
        ));
    }
    Ok(format!("{serial:08X}"))
}

#[cfg(windows)]
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_rejects_oem_placeholders() {
        for value in [
            "",
            "00000000",
            "FFFFFFFF",
            "To Be Filled By O.E.M.",
            "Default string",
            "Unknown",
        ] {
            assert_eq!(normalize_identifier(value), None, "{value}");
        }
    }

    #[test]
    fn identity_contains_only_domain_separated_hashes() {
        let raw = RawHardware {
            system_uuid: Some("12345678-1234-1234-1234-123456789ABC".to_owned()),
            system_serial: Some("SYSTEM-SECRET".to_owned()),
            baseboard_serial: Some("BOARD-SECRET".to_owned()),
            chassis_serial: Some("CHASSIS-SECRET".to_owned()),
            system_volume: Some("A1B2C3D4".to_owned()),
            machine_guid: Some("GUID-SECRET-1234".to_owned()),
        };
        let identity = HardwareIdentity::from_raw(raw).unwrap();
        let serialized = serde_json::to_string(&identity).unwrap();

        assert_eq!(identity.schema, HARDWARE_SCHEMA_VERSION);
        assert_eq!(identity.strength, HardwareIdentityStrength::Strong);
        assert_eq!(identity.fingerprint.len(), 64);
        assert!(identity
            .components
            .iter()
            .all(|component| component.digest.len() == 64));
        assert!(!serialized.contains("SECRET"));
        assert!(!serialized.contains("12345678-1234"));
    }

    #[test]
    fn smbios_parser_extracts_system_board_and_chassis_identity() {
        let mut table = vec![1, 24, 0, 0, 1, 2, 3, 4];
        table.extend(1u8..=16);
        table.extend(b"Maker\0Product\0Version\0SYS-123456\0\0");
        table.extend([2, 8, 1, 0, 1, 2, 3, 4]);
        table.extend(b"Maker\0Board\0Version\0BOARD-123456\0\0");
        table.extend([3, 8, 2, 0, 1, 2, 3, 4]);
        table.extend(b"Maker\0Chassis\0Version\0CHASSIS-123456\0\0");
        table.extend([127, 4, 3, 0, 0, 0]);

        let parsed = parse_smbios_table(&table);
        assert_eq!(
            parsed.system_uuid,
            Some(hex_lower(&(1u8..=16).collect::<Vec<_>>()))
        );
        assert_eq!(parsed.system_serial.as_deref(), Some("SYS-123456"));
        assert_eq!(parsed.baseboard_serial.as_deref(), Some("BOARD-123456"));
        assert_eq!(parsed.chassis_serial.as_deref(), Some("CHASSIS-123456"));
    }

    #[test]
    fn fingerprint_is_deterministic_and_domain_separated() {
        let first = domain_hash("machine", "VALUE-123");
        let second = domain_hash("machine", "VALUE-123");
        let component = domain_hash("component/system-uuid", "VALUE-123");
        assert_eq!(first, second);
        assert_ne!(first, component);
    }

    #[test]
    fn offline_match_tolerates_only_a_strong_majority_change() {
        let components = [
            HardwareComponentKind::SystemUuid,
            HardwareComponentKind::SystemSerial,
            HardwareComponentKind::BaseboardSerial,
            HardwareComponentKind::ChassisSerial,
            HardwareComponentKind::MachineGuid,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| HardwareComponent {
            kind,
            digest: format!("digest-{index}"),
        })
        .collect::<Vec<_>>();
        let changed_primary = HardwareIdentity {
            schema: HARDWARE_SCHEMA_VERSION.to_owned(),
            fingerprint: "new-primary".to_owned(),
            components: components[1..].to_vec(),
            strength: HardwareIdentityStrength::Strong,
        };
        assert!(matches_stored_hardware(
            HARDWARE_SCHEMA_VERSION,
            "old-primary",
            &components,
            &changed_primary
        ));

        let weak_match = HardwareIdentity {
            components: components[3..].to_vec(),
            ..changed_primary
        };
        assert!(!matches_stored_hardware(
            HARDWARE_SCHEMA_VERSION,
            "old-primary",
            &components,
            &weak_match
        ));
    }
}
