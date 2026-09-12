use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::DateTime;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fmt;
use zeroize::Zeroize;

pub const MACHINE_FILE_ALGORITHM: &str = "aes-256-gcm+ed25519";
const MAX_CERTIFICATE_SIZE: usize = 16 * 1024 * 1024;
const MIN_TTL_SECONDS: i64 = 60 * 60;
const CLOCK_TOLERANCE_SECONDS: i64 = 5 * 60;

pub struct MachineFileContext<'a> {
    pub account_id: &'a str,
    pub product_id: &'a str,
    pub policy_id: &'a str,
    pub license_id: &'a str,
    pub machine_id: &'a str,
    pub fingerprint: &'a str,
    pub license_key: &'a str,
    pub now: i64,
    pub maximum_ttl: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VerifiedMachineFile {
    pub issued_at: i64,
    pub expires_at: i64,
    pub ttl: i64,
}

#[derive(Clone)]
pub struct MachineCertificateVerifier {
    public_key: VerifyingKey,
}

impl MachineCertificateVerifier {
    pub fn from_encoded_public_key(public_key: &str) -> Result<Self, MachineCertificateError> {
        let bytes = decode_public_key(public_key)?;
        let public_key = VerifyingKey::from_bytes(&bytes)
            .map_err(|_| MachineCertificateError::InvalidPublicKey)?;
        Ok(Self { public_key })
    }

    pub fn verify_and_decrypt(
        &self,
        certificate: &[u8],
        expected: &MachineFileContext<'_>,
    ) -> Result<VerifiedMachineFile, MachineCertificateError> {
        if certificate.is_empty() || certificate.len() > MAX_CERTIFICATE_SIZE {
            return Err(MachineCertificateError::InvalidCertificate);
        }
        validate_context(expected)?;

        let payload = decode_certificate(certificate)?;
        if payload.alg != MACHINE_FILE_ALGORITHM {
            return Err(MachineCertificateError::WrongAlgorithm);
        }
        verify_payload_signature(&self.public_key, &payload)?;

        let mut plaintext = decrypt_machine_payload(&payload.enc, expected)?;
        let decoded = serde_json::from_slice::<Value>(&plaintext)
            .map_err(|_| MachineCertificateError::InvalidPayload);
        plaintext.zeroize();
        let decoded = decoded?;
        assert_machine_context(&decoded, expected)
    }
}

#[derive(Deserialize)]
struct CertificatePayload {
    enc: String,
    sig: String,
    alg: String,
}

fn decode_certificate(certificate: &[u8]) -> Result<CertificatePayload, MachineCertificateError> {
    let text = std::str::from_utf8(certificate)
        .map_err(|_| MachineCertificateError::InvalidCertificate)?;
    let normalized = text.replace("\r\n", "\n");
    let mut lines = normalized.lines();
    if lines.next() != Some("-----BEGIN MACHINE FILE-----") {
        return Err(MachineCertificateError::InvalidCertificate);
    }

    let mut encoded = String::new();
    let mut found_footer = false;
    for line in lines.by_ref() {
        if line == "-----END MACHINE FILE-----" {
            found_footer = true;
            break;
        }
        if !line
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "+/=".contains(character))
        {
            return Err(MachineCertificateError::InvalidCertificate);
        }
        encoded.push_str(line);
    }
    if !found_footer || encoded.is_empty() || lines.any(|line| !line.is_empty()) {
        return Err(MachineCertificateError::InvalidCertificate);
    }

    let decoded = STANDARD
        .decode(encoded)
        .map_err(|_| MachineCertificateError::InvalidCertificate)?;
    serde_json::from_slice(&decoded).map_err(|_| MachineCertificateError::InvalidCertificate)
}

fn verify_payload_signature(
    public_key: &VerifyingKey,
    payload: &CertificatePayload,
) -> Result<(), MachineCertificateError> {
    let signature = STANDARD
        .decode(&payload.sig)
        .map_err(|_| MachineCertificateError::InvalidSignature)?;
    let signature: [u8; 64] = signature
        .try_into()
        .map_err(|_| MachineCertificateError::InvalidSignature)?;
    let signature = Signature::from_bytes(&signature);
    public_key
        .verify(format!("machine/{}", payload.enc).as_bytes(), &signature)
        .map_err(|_| MachineCertificateError::InvalidSignature)
}

fn decrypt_machine_payload(
    encoded: &str,
    expected: &MachineFileContext<'_>,
) -> Result<Vec<u8>, MachineCertificateError> {
    let parts = encoded.split('.').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err(MachineCertificateError::InvalidEncryption);
    }
    let mut ciphertext = STANDARD
        .decode(parts[0])
        .map_err(|_| MachineCertificateError::InvalidEncryption)?;
    let iv = STANDARD
        .decode(parts[1])
        .map_err(|_| MachineCertificateError::InvalidEncryption)?;
    let tag = STANDARD
        .decode(parts[2])
        .map_err(|_| MachineCertificateError::InvalidEncryption)?;
    if iv.len() != 12 || tag.len() != 16 {
        ciphertext.zeroize();
        return Err(MachineCertificateError::InvalidEncryption);
    }
    ciphertext.extend_from_slice(&tag);

    let mut secret_input =
        Vec::with_capacity(expected.license_key.len() + expected.fingerprint.len());
    secret_input.extend_from_slice(expected.license_key.as_bytes());
    secret_input.extend_from_slice(expected.fingerprint.as_bytes());
    // Формат Keygen machine file ТРЕБУЕТ SHA-256(license_key + fingerprint) как
    // AES-256-GCM key (https://keygen.sh/docs/api/cryptography/ — "Hashing these
    // values with SHA256 is required"). Это требование протокола, а не упрощение:
    // любой другой KDF (HKDF/PBKDF2) сломает расшифровку файлов, выданных сервером.
    // Вход высокоэнтропийный (серверный ключ + SHA-256 fingerprint), поэтому
    // однократный SHA-256 криптографически состоятелен.
    let mut secret = Sha256::digest(&secret_input);
    secret_input.zeroize();

    let cipher = Aes256Gcm::new_from_slice(&secret)
        .map_err(|_| MachineCertificateError::InvalidEncryption)?;
    let nonce =
        Nonce::try_from(iv.as_slice()).map_err(|_| MachineCertificateError::InvalidEncryption)?;
    let decrypted = cipher
        .decrypt(&nonce, ciphertext.as_ref())
        .map_err(|_| MachineCertificateError::DecryptionFailed);
    secret.zeroize();
    ciphertext.zeroize();
    decrypted
}

fn validate_context(expected: &MachineFileContext<'_>) -> Result<(), MachineCertificateError> {
    if [
        expected.account_id,
        expected.product_id,
        expected.policy_id,
        expected.license_id,
        expected.machine_id,
        expected.fingerprint,
        expected.license_key,
    ]
    .iter()
    .any(|value| value.is_empty())
        || expected.maximum_ttl < MIN_TTL_SECONDS
    {
        return Err(MachineCertificateError::InvalidContext);
    }
    Ok(())
}

fn assert_machine_context(
    value: &Value,
    expected: &MachineFileContext<'_>,
) -> Result<VerifiedMachineFile, MachineCertificateError> {
    assert_string(value, &["data", "type"], "machines")?;
    assert_string(value, &["data", "id"], expected.machine_id)?;
    assert_string(
        value,
        &["data", "attributes", "fingerprint"],
        expected.fingerprint,
    )?;
    assert_string(
        value,
        &["data", "relationships", "account", "data", "id"],
        expected.account_id,
    )?;
    assert_string(
        value,
        &["data", "relationships", "product", "data", "id"],
        expected.product_id,
    )?;
    assert_string(
        value,
        &["data", "relationships", "license", "data", "id"],
        expected.license_id,
    )?;

    let included = value
        .get("included")
        .and_then(Value::as_array)
        .ok_or(MachineCertificateError::ContextMismatch)?;
    let license = included
        .iter()
        .find(|item| {
            item.get("type").and_then(Value::as_str) == Some("licenses")
                && item.get("id").and_then(Value::as_str) == Some(expected.license_id)
        })
        .ok_or(MachineCertificateError::ContextMismatch)?;
    assert_string(
        license,
        &["relationships", "policy", "data", "id"],
        expected.policy_id,
    )?;

    let issued = value_at(value, &["meta", "issued"])
        .and_then(Value::as_str)
        .and_then(parse_timestamp)
        .ok_or(MachineCertificateError::InvalidTime)?;
    let expiry = value_at(value, &["meta", "expiry"])
        .and_then(Value::as_str)
        .and_then(parse_timestamp)
        .ok_or(MachineCertificateError::InvalidTime)?;
    let ttl = value_at(value, &["meta", "ttl"])
        .and_then(Value::as_i64)
        .ok_or(MachineCertificateError::InvalidTime)?;

    if ttl < MIN_TTL_SECONDS
        || ttl > expected.maximum_ttl
        || expiry <= issued
        || (expiry - issued - ttl).abs() > 2
        || issued > expected.now + CLOCK_TOLERANCE_SECONDS
        || expected.now >= expiry
    {
        return Err(MachineCertificateError::InvalidTime);
    }

    Ok(VerifiedMachineFile {
        issued_at: issued,
        expires_at: expiry,
        ttl,
    })
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    path.iter().try_fold(value, |current, key| current.get(key))
}

fn assert_string(
    value: &Value,
    path: &[&str],
    expected: &str,
) -> Result<(), MachineCertificateError> {
    if value_at(value, path).and_then(Value::as_str) == Some(expected) {
        Ok(())
    } else {
        Err(MachineCertificateError::ContextMismatch)
    }
}

fn parse_timestamp(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp())
}

fn decode_public_key(encoded: &str) -> Result<[u8; 32], MachineCertificateError> {
    let encoded = encoded.trim();
    let decoded = if encoded.len() == 64 && encoded.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        (0..32)
            .map(|index| u8::from_str_radix(&encoded[index * 2..index * 2 + 2], 16))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| MachineCertificateError::InvalidPublicKey)?
    } else {
        STANDARD
            .decode(encoded)
            .map_err(|_| MachineCertificateError::InvalidPublicKey)?
    };
    decoded
        .try_into()
        .map_err(|_| MachineCertificateError::InvalidPublicKey)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MachineCertificateError {
    InvalidPublicKey,
    InvalidCertificate,
    WrongAlgorithm,
    InvalidSignature,
    InvalidEncryption,
    DecryptionFailed,
    InvalidPayload,
    InvalidContext,
    ContextMismatch,
    InvalidTime,
}

impl fmt::Display for MachineCertificateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPublicKey => "machine file public key is invalid",
            Self::InvalidCertificate => "machine file certificate is invalid",
            Self::WrongAlgorithm => "machine file algorithm is unexpected",
            Self::InvalidSignature => "machine file signature is invalid",
            Self::InvalidEncryption => "machine file encryption format is invalid",
            Self::DecryptionFailed => "machine file decryption failed",
            Self::InvalidPayload => "machine file payload is invalid",
            Self::InvalidContext => "expected machine context is invalid",
            Self::ContextMismatch => "machine file does not match the expected context",
            Self::InvalidTime => "machine file time bounds are invalid",
        })
    }
}

impl std::error::Error for MachineCertificateError {}

#[cfg(test)]
mod tests {
    use super::*;
    use aes_gcm::aead::Aead;
    use ed25519_dalek::{Signer, SigningKey};

    const NOW: i64 = 1_700_000_100;

    fn context<'a>(fingerprint: &'a str) -> MachineFileContext<'a> {
        MachineFileContext {
            account_id: "account-1",
            product_id: "product-1",
            policy_id: "policy-1",
            license_id: "license-1",
            machine_id: "machine-1",
            fingerprint,
            license_key: "LICENSE-KEY-1",
            now: NOW,
            maximum_ttl: 7 * 24 * 60 * 60,
        }
    }

    fn certificate() -> (MachineCertificateVerifier, Vec<u8>) {
        let plaintext = br#"{
          "data": {
            "type": "machines",
            "id": "machine-1",
            "attributes": {"fingerprint": "fingerprint-1"},
            "relationships": {
              "account": {"data": {"id": "account-1"}},
              "product": {"data": {"id": "product-1"}},
              "license": {"data": {"id": "license-1"}}
            }
          },
          "included": [{
            "type": "licenses",
            "id": "license-1",
            "relationships": {"policy": {"data": {"id": "policy-1"}}}
          }],
          "meta": {
            "issued": "2023-11-14T22:13:20Z",
            "expiry": "2023-11-15T22:13:20Z",
            "ttl": 86400
          }
        }"#;
        let mut input = b"LICENSE-KEY-1".to_vec();
        input.extend_from_slice(b"fingerprint-1");
        let secret = Sha256::digest(&input);
        let cipher = Aes256Gcm::new_from_slice(&secret).unwrap();
        let iv = [9u8; 12];
        let nonce = Nonce::try_from(iv.as_slice()).unwrap();
        let mut encrypted = cipher.encrypt(&nonce, plaintext.as_ref()).unwrap();
        let tag = encrypted.split_off(encrypted.len() - 16);
        let enc = format!(
            "{}.{}.{}",
            STANDARD.encode(encrypted),
            STANDARD.encode(iv),
            STANDARD.encode(tag)
        );

        let signing_key = SigningKey::from_bytes(&[3u8; 32]);
        let signature = signing_key.sign(format!("machine/{enc}").as_bytes());
        let payload = serde_json::json!({
            "enc": enc,
            "sig": STANDARD.encode(signature.to_bytes()),
            "alg": MACHINE_FILE_ALGORITHM,
        });
        let certificate = format!(
            "-----BEGIN MACHINE FILE-----\n{}\n-----END MACHINE FILE-----\n",
            STANDARD.encode(serde_json::to_vec(&payload).unwrap())
        )
        .into_bytes();
        let verifier = MachineCertificateVerifier::from_encoded_public_key(
            &STANDARD.encode(signing_key.verifying_key().as_bytes()),
        )
        .unwrap();
        (verifier, certificate)
    }

    #[test]
    fn verifies_decrypts_and_asserts_full_context() {
        let (verifier, certificate) = certificate();
        let verified = verifier
            .verify_and_decrypt(&certificate, &context("fingerprint-1"))
            .unwrap();
        assert_eq!(verified.issued_at, 1_700_000_000);
        assert_eq!(verified.expires_at, 1_700_086_400);
        assert_eq!(verified.ttl, 86_400);
    }

    #[test]
    fn rejects_wrong_machine_or_modified_certificate() {
        let (verifier, mut certificate) = certificate();
        assert_eq!(
            verifier.verify_and_decrypt(&certificate, &context("different-fingerprint")),
            Err(MachineCertificateError::DecryptionFailed)
        );

        let position = certificate.iter().position(|byte| *byte == b'A').unwrap();
        certificate[position] = b'B';
        assert!(verifier
            .verify_and_decrypt(&certificate, &context("fingerprint-1"))
            .is_err());
    }
}
