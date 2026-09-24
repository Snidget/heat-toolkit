//! Headless E2E probe for the HEAT3 licensing stack against a live Keygen
//! instance (architecture plan sections 16-17, scenario matrix 1-17).
//!
//! Unlike the GUI client, this binary is configured at runtime from the
//! environment, so one build works for staging and production:
//!   HEAT3_E2E_API_URL, HEAT3_E2E_ACCOUNT_ID, HEAT3_E2E_PRODUCT_ID,
//!   HEAT3_E2E_POLICY_ID, HEAT3_E2E_PUBLIC_KEY,
//!   HEAT3_E2E_OFFLINE_TTL (optional, default 604800)
//!
//! Commands (exit code 0 = PASS, 2 = expected failure, 1 = unexpected error):
//!   validate <key>                          scenario 1/17
//!   activate <key> <cert-out>               scenarios 1, 2, 3, 9
//!   checkout <key> <license-id> <machine-id> scenarios 5, 6, 7
//!   checkin <key> <license-id>              online refresh (scenario 3)
//!   deactivate <key> <license-id> <machine-id> scenario 16
//!   verify-offline <key> <license-id> <machine-id> <cert-file>
//!                                           scenarios 11, 15 (tamper -> exit 2)
//!   expect-network-failure <key> <license-id> <machine-id>
//!                                           scenarios 13, 14 (error mapping)

use std::env;
use std::fs;
use std::process::ExitCode;

use heat3_povorotnik::licensing::{collect_hardware_identity, HardwareIdentity};
use heat3_povorotnik::licensing::{
    KeygenClient, KeygenClientError, KeygenConfig, MachineCertificateVerifier, MachineFileContext,
    ValidationCode,
};

fn env_var(name: &str) -> Result<String, String> {
    env::var(name).map_err(|_| format!("Missing environment variable {name}"))
}

struct Env {
    api_url: String,
    account_id: String,
    product_id: String,
    policy_id: String,
    public_key: String,
    offline_ttl: i64,
}

fn load_env() -> Result<Env, String> {
    let offline_ttl = env::var("HEAT3_E2E_OFFLINE_TTL")
        .unwrap_or_else(|_| "604800".to_owned())
        .parse::<i64>()
        .map_err(|_| "HEAT3_E2E_OFFLINE_TTL is not a number".to_owned())?;
    Ok(Env {
        api_url: env_var("HEAT3_E2E_API_URL")?,
        account_id: env_var("HEAT3_E2E_ACCOUNT_ID")?,
        product_id: env_var("HEAT3_E2E_PRODUCT_ID")?,
        policy_id: env_var("HEAT3_E2E_POLICY_ID")?,
        public_key: env_var("HEAT3_E2E_PUBLIC_KEY")?,
        offline_ttl,
    })
}

fn build_client(env: &Env) -> Result<KeygenClient, String> {
    let config = KeygenConfig::new(
        &env.api_url,
        &env.account_id,
        &env.product_id,
        &env.policy_id,
        &env.public_key,
        env.offline_ttl,
    )
    .map_err(|error| format!("invalid KeygenConfig: {error}"))?;
    KeygenClient::new(config).map_err(|error| format!("client init failed: {error}"))
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(message) => {
            eprintln!("FATAL: {message}");
            ExitCode::from(1)
        }
    }
}

fn run() -> Result<u8, String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let Some(command) = args.first().map(String::as_str) else {
        return Err(usage());
    };
    let env = load_env()?;
    let client = build_client(&env)?;
    let hardware =
        collect_hardware_identity().map_err(|error| format!("hardware identity: {error}"))?;
    match command {
        "validate" => cmd_validate(&client, &hardware, arg(&args, 1)?),
        "activate" => cmd_activate(&client, &hardware, arg(&args, 1)?, arg(&args, 2)?),
        "checkout" => cmd_checkout(
            &client,
            &hardware,
            arg(&args, 1)?,
            arg(&args, 2)?,
            arg(&args, 3)?,
        ),
        "checkin" => cmd_checkin(&client, arg(&args, 1)?, arg(&args, 2)?),
        "deactivate" => cmd_deactivate(
            &client,
            &hardware,
            arg(&args, 1)?,
            arg(&args, 2)?,
            arg(&args, 3)?,
        ),
        "verify-offline" => cmd_verify_offline(
            &env,
            &hardware,
            arg(&args, 1)?,
            arg(&args, 2)?,
            arg(&args, 3)?,
            arg(&args, 4)?,
        ),
        "expect-network-failure" => cmd_expect_network_failure(
            &client,
            &hardware,
            arg(&args, 1)?,
            arg(&args, 2)?,
            arg(&args, 3)?,
        ),
        _ => Err(usage()),
    }
}

fn arg(args: &[String], index: usize) -> Result<String, String> {
    args.get(index)
        .cloned()
        .ok_or_else(|| format!("missing argument #{}: {}", index + 1, usage()))
}

fn usage() -> String {
    "usage: e2e_staging <validate|activate|checkout|checkin|deactivate|verify-offline|expect-network-failure> ..."
        .to_owned()
}

fn cmd_validate(
    client: &KeygenClient,
    hardware: &HardwareIdentity,
    license_key: String,
) -> Result<u8, String> {
    let result = client
        .validate_key(&license_key, hardware)
        .map_err(|error| format!("validate_key failed: {error}"))?;
    println!(
        "VALIDATION valid={} code={:?} server_time={}",
        result.valid, result.code, result.server_time
    );
    Ok(if result.valid { 0 } else { 2 })
}

fn cmd_activate(
    client: &KeygenClient,
    hardware: &HardwareIdentity,
    license_key: String,
    cert_out: String,
) -> Result<u8, String> {
    // Mirror production activation semantics: a strict node-locked policy
    // answers the first validation for a legitimate, not-yet-activated license
    // with NO_MACHINE. Find-before-create keeps activation idempotent, then a
    // final revalidation must be VALID.
    let validation = client
        .validate_key(&license_key, hardware)
        .map_err(|error| format!("validate_key failed: {error}"))?;
    if !matches!(
        validation.code,
        ValidationCode::Valid | ValidationCode::NoMachine
    ) {
        return Err(format!(
            "license is not in an activatable state: {:?}",
            validation.code
        ));
    }
    let license_id = validation
        .license_id
        .ok_or_else(|| "server returned no license_id".to_owned())?;
    let machine = match client
        .find_machine(&license_key, &license_id, &hardware.fingerprint)
        .map_err(|error| format!("find_machine failed: {error}"))?
    {
        Some(machine) => machine,
        None => client
            .activate_machine(&license_key, &license_id, hardware, "E2E-Staging-Harness")
            .map_err(|error| format!("activate_machine failed: {error}"))?,
    };
    let revalidation = client
        .validate_key(&license_key, hardware)
        .map_err(|error| format!("revalidate_key failed: {error}"))?;
    if !revalidation.valid {
        return Err(format!(
            "license is not valid after machine creation: {:?}",
            revalidation.code
        ));
    }
    let checkout = client
        .checkout_machine(
            &license_key,
            &license_id,
            &machine.machine_id,
            &hardware.fingerprint,
        )
        .map_err(|error| format!("checkout_machine failed: {error}"))?;
    println!(
        "ACTIVATION license_id={} machine_id={} ttl={} expires_at={}",
        license_id, machine.machine_id, checkout.verified.ttl, checkout.verified.expires_at
    );
    fs::write(&cert_out, &checkout.certificate)
        .map_err(|error| format!("cannot write certificate to {cert_out}: {error}"))?;
    println!(
        "CERTIFICATE_SAVED path={} bytes={}",
        cert_out,
        checkout.certificate.len()
    );
    Ok(0)
}

fn cmd_checkout(
    client: &KeygenClient,
    hardware: &HardwareIdentity,
    license_key: String,
    license_id: String,
    machine_id: String,
) -> Result<u8, String> {
    let result = client.checkout_machine(
        &license_key,
        &license_id,
        &machine_id,
        &hardware.fingerprint,
    );
    match result {
        Ok(checkout) => {
            println!(
                "CHECKOUT_OK ttl={} expires_at={}",
                checkout.verified.ttl, checkout.verified.expires_at
            );
            Ok(0)
        }
        Err(error) => {
            println!("CHECKOUT_FAILED error={error}");
            Ok(2)
        }
    }
}

fn cmd_checkin(
    client: &KeygenClient,
    license_key: String,
    license_id: String,
) -> Result<u8, String> {
    let server_time = client
        .check_in(&license_key, &license_id)
        .map_err(|error| format!("check_in failed: {error}"))?;
    println!("CHECKIN_OK server_time={server_time}");
    Ok(0)
}

fn cmd_deactivate(
    client: &KeygenClient,
    hardware: &HardwareIdentity,
    license_key: String,
    license_id: String,
    machine_id: String,
) -> Result<u8, String> {
    client
        .deactivate_machine(&license_key, &machine_id)
        .map_err(|error| format!("deactivate_machine failed: {error}"))?;
    let found = client
        .find_machine(&license_key, &license_id, &hardware.fingerprint)
        .map_err(|error| format!("find_machine after deactivate failed: {error}"))?;
    if found.is_some() {
        return Err("machine still exists after deactivation".to_owned());
    }
    println!("DEACTIVATE_OK machine removed");
    Ok(0)
}

fn cmd_verify_offline(
    env: &Env,
    hardware: &HardwareIdentity,
    license_key: String,
    license_id: String,
    machine_id: String,
    cert_file: String,
) -> Result<u8, String> {
    let bytes =
        fs::read(&cert_file).map_err(|error| format!("cannot read {cert_file}: {error}"))?;
    let verifier = MachineCertificateVerifier::from_encoded_public_key(&env.public_key)
        .map_err(|error| format!("bad public key: {error}"))?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|_| "system clock before epoch".to_owned())?
        .as_secs() as i64;
    let verified = verifier.verify_and_decrypt(
        &bytes,
        &MachineFileContext {
            account_id: &env.account_id,
            product_id: &env.product_id,
            policy_id: &env.policy_id,
            license_id: &license_id,
            machine_id: &machine_id,
            fingerprint: &hardware.fingerprint,
            license_key: &license_key,
            now,
            maximum_ttl: env.offline_ttl,
        },
    );
    match verified {
        Ok(file) => {
            println!(
                "OFFLINE_VERIFY_OK issued_at={} expires_at={} ttl={}",
                file.issued_at, file.expires_at, file.ttl
            );
            Ok(0)
        }
        Err(error) => {
            println!("OFFLINE_VERIFY_FAILED error={error}");
            Ok(2)
        }
    }
}

fn cmd_expect_network_failure(
    client: &KeygenClient,
    hardware: &HardwareIdentity,
    license_key: String,
    license_id: String,
    machine_id: String,
) -> Result<u8, String> {
    let result = client.checkout_machine(
        &license_key,
        &license_id,
        &machine_id,
        &hardware.fingerprint,
    );
    match result {
        Ok(_) => Err("network failure expected but checkout succeeded".to_owned()),
        Err(
            KeygenClientError::Timeout
            | KeygenClientError::Connection
            | KeygenClientError::Transport,
        ) => {
            println!("NETWORK_FAILURE_AS_EXPECTED");
            Ok(0)
        }
        Err(KeygenClientError::Api { status, .. }) if (500..=599).contains(&status) => {
            println!("NETWORK_FAILURE_AS_EXPECTED status={status}");
            Ok(0)
        }
        Err(error) => Err(format!("unexpected error kind: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_mentions_all_commands() {
        let text = usage();
        for command in [
            "validate",
            "activate",
            "checkout",
            "checkin",
            "deactivate",
            "verify-offline",
            "expect-network-failure",
        ] {
            assert!(text.contains(command), "usage must mention {command}");
        }
    }

    #[test]
    fn arg_reports_missing_position() {
        let error = arg(&["validate".to_owned()], 1).unwrap_err();
        assert!(error.starts_with("missing argument #2"));
    }
}
