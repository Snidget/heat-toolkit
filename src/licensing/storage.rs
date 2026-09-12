use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

use super::HardwareComponent;

pub const STORAGE_SCHEMA_VERSION: u32 = 2;
const ENVELOPE_MAGIC: &[u8; 8] = b"H3LICV2\0";
const LEGACY_STORAGE_SCHEMA_VERSION: u32 = 1;
const LEGACY_ENVELOPE_MAGIC: &[u8; 8] = b"H3LICV1\0";
const ENVELOPE_HEADER_LEN: usize = 8 + 4 + 4 + 32;
const MAX_ENVELOPE_SIZE: usize = 16 * 1024 * 1024;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Deserialize, Eq, PartialEq, Serialize)]
pub struct SecureLicenseRecord {
    schema_version: u32,
    license_key: String,
    license_id: String,
    machine_id: String,
    activation_fingerprint: String,
    fingerprint_schema_version: String,
    hashed_components: Vec<HardwareComponent>,
    encrypted_signed_machine_file: Vec<u8>,
    last_successful_online_server_time: i64,
    max_observed_trusted_time: i64,
    offline_valid_until: i64,
    #[serde(default)]
    authoritative_block: Option<String>,
    #[serde(default)]
    deactivation_pending: bool,
    last_validation_code: Option<String>,
    last_error_class: Option<String>,
}

impl Drop for SecureLicenseRecord {
    fn drop(&mut self) {
        self.license_key.zeroize();
        self.activation_fingerprint.zeroize();
        self.encrypted_signed_machine_file.zeroize();
    }
}

impl SecureLicenseRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        license_key: impl Into<String>,
        license_id: impl Into<String>,
        machine_id: impl Into<String>,
        activation_fingerprint: impl Into<String>,
        fingerprint_schema_version: impl Into<String>,
        hashed_components: Vec<HardwareComponent>,
        encrypted_signed_machine_file: Vec<u8>,
        last_successful_online_server_time: i64,
        max_observed_trusted_time: i64,
        offline_valid_until: i64,
    ) -> Result<Self, StorageError> {
        let record = Self {
            schema_version: STORAGE_SCHEMA_VERSION,
            license_key: license_key.into(),
            license_id: license_id.into(),
            machine_id: machine_id.into(),
            activation_fingerprint: activation_fingerprint.into(),
            fingerprint_schema_version: fingerprint_schema_version.into(),
            hashed_components,
            encrypted_signed_machine_file,
            last_successful_online_server_time,
            max_observed_trusted_time,
            offline_valid_until,
            authoritative_block: None,
            deactivation_pending: false,
            last_validation_code: None,
            last_error_class: None,
        };
        record.validate()?;
        Ok(record)
    }

    pub fn license_key(&self) -> &str {
        &self.license_key
    }

    pub fn license_id(&self) -> &str {
        &self.license_id
    }

    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    pub fn activation_fingerprint(&self) -> &str {
        &self.activation_fingerprint
    }

    pub fn fingerprint_schema_version(&self) -> &str {
        &self.fingerprint_schema_version
    }

    pub fn hashed_components(&self) -> &[HardwareComponent] {
        &self.hashed_components
    }

    pub fn encrypted_signed_machine_file(&self) -> &[u8] {
        &self.encrypted_signed_machine_file
    }

    pub fn last_successful_online_server_time(&self) -> i64 {
        self.last_successful_online_server_time
    }

    pub fn max_observed_trusted_time(&self) -> i64 {
        self.max_observed_trusted_time
    }

    pub fn offline_valid_until(&self) -> i64 {
        self.offline_valid_until
    }

    pub fn set_validation_result(
        &mut self,
        validation_code: Option<String>,
        error_class: Option<String>,
    ) {
        self.last_validation_code = validation_code;
        self.last_error_class = error_class;
    }

    pub fn authoritative_block(&self) -> Option<&str> {
        self.authoritative_block.as_deref()
    }

    pub fn set_authoritative_block(&mut self, value: Option<String>) {
        self.authoritative_block = value;
    }

    pub fn deactivation_pending(&self) -> bool {
        self.deactivation_pending
    }

    pub fn set_deactivation_pending(&mut self, value: bool) {
        self.deactivation_pending = value;
    }

    pub fn observe_time(&mut self, unix_seconds: i64) {
        self.max_observed_trusted_time = self.max_observed_trusted_time.max(unix_seconds);
    }

    fn validate(&self) -> Result<(), StorageError> {
        if self.schema_version != STORAGE_SCHEMA_VERSION
            || self.license_key.trim().is_empty()
            || self.license_id.trim().is_empty()
            || self.machine_id.trim().is_empty()
            || self.activation_fingerprint.trim().is_empty()
            || self.fingerprint_schema_version.trim().is_empty()
            || self.encrypted_signed_machine_file.is_empty()
            || self.offline_valid_until <= self.last_successful_online_server_time
            || self.max_observed_trusted_time < self.last_successful_online_server_time
        {
            return Err(StorageError::InvalidRecord);
        }
        Ok(())
    }

    fn upgrade_legacy_schema(&mut self) {
        if self.schema_version == LEGACY_STORAGE_SCHEMA_VERSION {
            self.schema_version = STORAGE_SCHEMA_VERSION;
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageError {
    UnsupportedPlatform,
    MissingLocalAppData,
    InvalidRecord,
    InvalidEnvelope,
    EnvelopeTooLarge,
    ConcurrentModification,
    Dpapi(u32),
    Io(io::ErrorKind),
    Serialization,
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => f.write_str("secure storage requires Windows"),
            Self::MissingLocalAppData => f.write_str("LOCALAPPDATA is unavailable"),
            Self::InvalidRecord => f.write_str("license record is invalid"),
            Self::InvalidEnvelope => f.write_str("license envelope is invalid"),
            Self::EnvelopeTooLarge => f.write_str("license envelope exceeds the size limit"),
            Self::ConcurrentModification => {
                f.write_str("license storage changed during the current operation")
            }
            Self::Dpapi(code) => write!(f, "DPAPI error {code}"),
            Self::Io(kind) => write!(f, "license storage I/O error: {kind:?}"),
            Self::Serialization => f.write_str("license record serialization failed"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<io::Error> for StorageError {
    fn from(error: io::Error) -> Self {
        Self::Io(error.kind())
    }
}

#[derive(Clone, Debug)]
pub struct SecureStore {
    path: PathBuf,
}

impl SecureStore {
    pub fn for_current_user() -> Result<Self, StorageError> {
        let local_app_data =
            std::env::var_os("LOCALAPPDATA").ok_or(StorageError::MissingLocalAppData)?;
        Ok(Self::at(
            PathBuf::from(local_app_data)
                .join("HEAT3")
                .join("Povorotnik")
                .join("license.v1.bin"),
        ))
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Option<SecureLicenseRecord>, StorageError> {
        self.with_exclusive_lock(|_| self.load_unlocked())
    }

    pub fn save(&self, record: &SecureLicenseRecord) -> Result<(), StorageError> {
        self.with_exclusive_lock(|_| self.save_unlocked(record))
    }

    pub fn delete(&self) -> Result<(), StorageError> {
        self.with_exclusive_lock(|_| self.delete_unlocked())
    }

    pub fn update<R, F>(&self, mutator: F) -> Result<R, StorageError>
    where
        F: FnOnce(
            Option<SecureLicenseRecord>,
        ) -> Result<(Option<SecureLicenseRecord>, R), StorageError>,
    {
        self.with_exclusive_lock(|_| {
            let current = self.load_unlocked()?;
            let (next, result) = mutator(current)?;
            match next {
                Some(record) => self.save_unlocked(&record)?,
                None => self.delete_unlocked()?,
            }
            Ok(result)
        })
    }

    fn load_unlocked(&self) -> Result<Option<SecureLicenseRecord>, StorageError> {
        let encrypted = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if encrypted.len() > MAX_ENVELOPE_SIZE {
            return Err(StorageError::EnvelopeTooLarge);
        }

        let (protected, needs_migration) = match decode_envelope(&encrypted) {
            Ok(protected) => (protected, false),
            Err(_) => (decode_legacy_envelope(&encrypted)?, true),
        };
        let mut plaintext = dpapi_unprotect(protected)?;
        let record = serde_json::from_slice::<SecureLicenseRecord>(&plaintext)
            .map_err(|_| StorageError::Serialization);
        plaintext.zeroize();

        let mut record = record?;
        if needs_migration || record.schema_version == LEGACY_STORAGE_SCHEMA_VERSION {
            record.upgrade_legacy_schema();
            record.validate()?;
            self.save_unlocked(&record)?;
            return Ok(Some(record));
        }
        record.validate()?;
        Ok(Some(record))
    }

    fn save_unlocked(&self, record: &SecureLicenseRecord) -> Result<(), StorageError> {
        record.validate()?;
        let mut plaintext = serde_json::to_vec(record).map_err(|_| StorageError::Serialization)?;
        let protected = dpapi_protect(&plaintext);
        plaintext.zeroize();
        let protected = protected?;
        let envelope = encode_envelope(&protected)?;

        let parent = self.path.parent().ok_or(StorageError::InvalidEnvelope)?;
        fs::create_dir_all(parent)?;
        atomic_write(&self.path, &envelope)
    }

    fn delete_unlocked(&self) -> Result<(), StorageError> {
        match fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.into()),
        }
    }

    fn with_exclusive_lock<R>(
        &self,
        operation: impl FnOnce(&File) -> Result<R, StorageError>,
    ) -> Result<R, StorageError> {
        let lock_path = self.lock_path();
        let parent = lock_path.parent().ok_or(StorageError::InvalidEnvelope)?;
        fs::create_dir_all(parent)?;
        let lock_file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        lock_file.lock_exclusive()?;
        let result = operation(&lock_file);
        let unlock_result = FileExt::unlock(&lock_file).map_err(StorageError::from);
        match (result, unlock_result) {
            (Ok(value), Ok(())) => Ok(value),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    fn lock_path(&self) -> PathBuf {
        let mut path = OsString::from(self.path.as_os_str());
        path.push(".lock");
        PathBuf::from(path)
    }
}

fn encode_envelope(protected: &[u8]) -> Result<Vec<u8>, StorageError> {
    if protected.is_empty() || protected.len() > MAX_ENVELOPE_SIZE - ENVELOPE_HEADER_LEN {
        return Err(StorageError::EnvelopeTooLarge);
    }
    let length = u32::try_from(protected.len()).map_err(|_| StorageError::EnvelopeTooLarge)?;
    let digest = Sha256::digest(protected);

    let mut envelope = Vec::with_capacity(ENVELOPE_HEADER_LEN + protected.len());
    envelope.extend_from_slice(ENVELOPE_MAGIC);
    envelope.extend_from_slice(&STORAGE_SCHEMA_VERSION.to_le_bytes());
    envelope.extend_from_slice(&length.to_le_bytes());
    envelope.extend_from_slice(&digest);
    envelope.extend_from_slice(protected);
    Ok(envelope)
}

fn decode_envelope(envelope: &[u8]) -> Result<&[u8], StorageError> {
    if envelope.len() < ENVELOPE_HEADER_LEN || envelope.len() > MAX_ENVELOPE_SIZE {
        return Err(StorageError::InvalidEnvelope);
    }
    if &envelope[..8] != ENVELOPE_MAGIC {
        return Err(StorageError::InvalidEnvelope);
    }
    let version = u32::from_le_bytes(
        envelope[8..12]
            .try_into()
            .map_err(|_| StorageError::InvalidEnvelope)?,
    );
    if version != STORAGE_SCHEMA_VERSION {
        return Err(StorageError::InvalidEnvelope);
    }
    let length = u32::from_le_bytes(
        envelope[12..16]
            .try_into()
            .map_err(|_| StorageError::InvalidEnvelope)?,
    ) as usize;
    if ENVELOPE_HEADER_LEN
        .checked_add(length)
        .filter(|expected| *expected == envelope.len())
        .is_none()
    {
        return Err(StorageError::InvalidEnvelope);
    }

    let protected = &envelope[ENVELOPE_HEADER_LEN..];
    let expected_digest = &envelope[16..48];
    if Sha256::digest(protected).as_slice() != expected_digest {
        return Err(StorageError::InvalidEnvelope);
    }
    Ok(protected)
}

fn decode_legacy_envelope(envelope: &[u8]) -> Result<&[u8], StorageError> {
    decode_envelope_with_header(
        envelope,
        LEGACY_ENVELOPE_MAGIC,
        LEGACY_STORAGE_SCHEMA_VERSION,
    )
}

fn decode_envelope_with_header<'a>(
    envelope: &'a [u8],
    magic: &[u8; 8],
    version: u32,
) -> Result<&'a [u8], StorageError> {
    if envelope.len() < ENVELOPE_HEADER_LEN || envelope.len() > MAX_ENVELOPE_SIZE {
        return Err(StorageError::InvalidEnvelope);
    }
    if &envelope[..8] != magic {
        return Err(StorageError::InvalidEnvelope);
    }
    let envelope_version = u32::from_le_bytes(
        envelope[8..12]
            .try_into()
            .map_err(|_| StorageError::InvalidEnvelope)?,
    );
    if envelope_version != version {
        return Err(StorageError::InvalidEnvelope);
    }
    let length = u32::from_le_bytes(
        envelope[12..16]
            .try_into()
            .map_err(|_| StorageError::InvalidEnvelope)?,
    ) as usize;
    if ENVELOPE_HEADER_LEN
        .checked_add(length)
        .filter(|expected| *expected == envelope.len())
        .is_none()
    {
        return Err(StorageError::InvalidEnvelope);
    }

    let protected = &envelope[ENVELOPE_HEADER_LEN..];
    let expected_digest = &envelope[16..48];
    if Sha256::digest(protected).as_slice() != expected_digest {
        return Err(StorageError::InvalidEnvelope);
    }
    Ok(protected)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    let parent = path.parent().ok_or(StorageError::InvalidEnvelope)?;
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| StorageError::InvalidEnvelope)?
        .as_nanos();
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".license-{}-{unique}-{sequence}.tmp",
        std::process::id()
    ));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        atomic_replace(&temporary, path)
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(windows)]
fn atomic_replace(source: &Path, destination: &Path) -> Result<(), StorageError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };

    let source = source
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = destination
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let ok = unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if ok == 0 {
        return Err(io::Error::last_os_error().into());
    }
    Ok(())
}

#[cfg(not(windows))]
fn atomic_replace(_source: &Path, _destination: &Path) -> Result<(), StorageError> {
    Err(StorageError::UnsupportedPlatform)
}

#[cfg(windows)]
fn dpapi_protect(plaintext: &[u8]) -> Result<Vec<u8>, StorageError> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptProtectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    const ENTROPY: &[u8] = b"HEAT3/Povorotnik/license-storage/v1";
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(plaintext.len()).map_err(|_| StorageError::EnvelopeTooLarge)?,
        pbData: plaintext.as_ptr().cast_mut(),
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptProtectData(
            &input,
            null(),
            &entropy,
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(StorageError::Dpapi(unsafe { GetLastError() }));
    }
    if output.pbData.is_null() || output.cbData == 0 {
        return Err(StorageError::InvalidEnvelope);
    }

    let protected =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        LocalFree(output.pbData.cast());
    }
    let _ = null_mut::<u8>();
    Ok(protected)
}

#[cfg(not(windows))]
fn dpapi_protect(_plaintext: &[u8]) -> Result<Vec<u8>, StorageError> {
    Err(StorageError::UnsupportedPlatform)
}

#[cfg(windows)]
fn dpapi_unprotect(protected: &[u8]) -> Result<Vec<u8>, StorageError> {
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::Foundation::{GetLastError, LocalFree};
    use windows_sys::Win32::Security::Cryptography::{
        CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
    };

    const ENTROPY: &[u8] = b"HEAT3/Povorotnik/license-storage/v1";
    let input = CRYPT_INTEGER_BLOB {
        cbData: u32::try_from(protected.len()).map_err(|_| StorageError::EnvelopeTooLarge)?,
        pbData: protected.as_ptr().cast_mut(),
    };
    let entropy = CRYPT_INTEGER_BLOB {
        cbData: ENTROPY.len() as u32,
        pbData: ENTROPY.as_ptr().cast_mut(),
    };
    let mut output = CRYPT_INTEGER_BLOB::default();
    let ok = unsafe {
        CryptUnprotectData(
            &input,
            null_mut(),
            &entropy,
            null(),
            null(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut output,
        )
    };
    if ok == 0 {
        return Err(StorageError::Dpapi(unsafe { GetLastError() }));
    }
    if output.pbData.is_null() || output.cbData == 0 {
        return Err(StorageError::InvalidEnvelope);
    }

    let mut plaintext =
        unsafe { std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec() };
    unsafe {
        std::ptr::write_bytes(output.pbData, 0, output.cbData as usize);
        LocalFree(output.pbData.cast());
    }
    if plaintext.is_empty() {
        plaintext.zeroize();
        return Err(StorageError::InvalidEnvelope);
    }
    Ok(plaintext)
}

#[cfg(not(windows))]
fn dpapi_unprotect(_protected: &[u8]) -> Result<Vec<u8>, StorageError> {
    Err(StorageError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::licensing::{HardwareComponentKind, HARDWARE_SCHEMA_VERSION};

    fn record() -> SecureLicenseRecord {
        SecureLicenseRecord::new(
            "KEY-SECRET-1234",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3, 4],
            1_000,
            1_000,
            2_000,
        )
        .unwrap()
    }

    #[test]
    fn envelope_detects_truncation_and_modification() {
        let mut envelope = encode_envelope(b"protected").unwrap();
        assert_eq!(decode_envelope(&envelope).unwrap(), b"protected");

        envelope.pop();
        assert_eq!(
            decode_envelope(&envelope),
            Err(StorageError::InvalidEnvelope)
        );

        let mut envelope = encode_envelope(b"protected").unwrap();
        *envelope.last_mut().unwrap() ^= 1;
        assert_eq!(
            decode_envelope(&envelope),
            Err(StorageError::InvalidEnvelope)
        );
    }

    #[test]
    fn record_rejects_invalid_time_range() {
        assert!(matches!(
            SecureLicenseRecord::new(
                "key",
                "license",
                "machine",
                "fingerprint",
                HARDWARE_SCHEMA_VERSION,
                Vec::new(),
                vec![1],
                2,
                2,
                2,
            ),
            Err(StorageError::InvalidRecord)
        ));
    }

    #[test]
    fn v1_envelope_is_rejected_after_schema_upgrade() {
        let protected = b"protected";
        let digest = Sha256::digest(protected);
        let mut envelope = Vec::new();
        envelope.extend_from_slice(b"H3LICV1\0");
        envelope.extend_from_slice(&1u32.to_le_bytes());
        envelope.extend_from_slice(&(protected.len() as u32).to_le_bytes());
        envelope.extend_from_slice(&digest);
        envelope.extend_from_slice(protected);

        assert_eq!(
            decode_envelope(&envelope),
            Err(StorageError::InvalidEnvelope)
        );
    }

    #[cfg(windows)]
    #[test]
    fn legacy_v1_record_is_migrated_on_load() {
        let directory = std::env::temp_dir().join(format!(
            "heat3-license-legacy-load-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let store = SecureStore::at(directory.join("license.v1.bin"));
        let mut legacy = record();
        legacy.schema_version = 1;
        let plaintext = serde_json::to_vec(&legacy).unwrap();
        let protected = dpapi_protect(&plaintext).unwrap();
        let digest = Sha256::digest(&protected);
        let mut envelope = Vec::new();
        envelope.extend_from_slice(b"H3LICV1\0");
        envelope.extend_from_slice(&1u32.to_le_bytes());
        envelope.extend_from_slice(&(protected.len() as u32).to_le_bytes());
        envelope.extend_from_slice(&digest);
        envelope.extend_from_slice(&protected);
        fs::write(store.path(), envelope).unwrap();

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.license_id(), "license-id");

        let migrated = fs::read(store.path()).unwrap();
        assert!(decode_envelope(&migrated).is_ok());

        store.delete().unwrap();
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(windows)]
    #[test]
    fn legacy_v1_migration_preserves_security_flags() {
        let directory = std::env::temp_dir().join(format!(
            "heat3-license-legacy-flags-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        let store = SecureStore::at(directory.join("license.v1.bin"));
        let mut legacy = record();
        legacy.schema_version = 1;
        legacy.set_authoritative_block(Some("suspended".to_owned()));
        legacy.set_deactivation_pending(true);
        let plaintext = serde_json::to_vec(&legacy).unwrap();
        let protected = dpapi_protect(&plaintext).unwrap();
        let digest = Sha256::digest(&protected);
        let mut envelope = Vec::new();
        envelope.extend_from_slice(b"H3LICV1\0");
        envelope.extend_from_slice(&1u32.to_le_bytes());
        envelope.extend_from_slice(&(protected.len() as u32).to_le_bytes());
        envelope.extend_from_slice(&digest);
        envelope.extend_from_slice(&protected);
        fs::write(store.path(), envelope).unwrap();

        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.authoritative_block(), Some("suspended"));
        assert!(loaded.deactivation_pending());

        store.delete().unwrap();
        let _ = fs::remove_dir_all(directory);
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_store_roundtrips_for_current_user() {
        let directory = std::env::temp_dir().join(format!(
            "heat3-license-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let store = SecureStore::at(directory.join("license.v1.bin"));
        let record = record();

        store.save(&record).unwrap();
        let loaded = store.load().unwrap().unwrap();
        assert_eq!(loaded.license_key(), "KEY-SECRET-1234");
        assert_eq!(loaded.license_id(), "license-id");
        assert_ne!(
            fs::read(store.path()).unwrap(),
            serde_json::to_vec(&record).unwrap()
        );

        store.delete().unwrap();
        let _ = fs::remove_dir_all(directory);
    }
}
