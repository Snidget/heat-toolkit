use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use zeroize::Zeroize;

use super::{
    collect_hardware_identity, mask_license_key, matches_stored_hardware, AccessDecision,
    KeygenClient, KeygenClientError, KeygenConfig, LicenseGate, LicenseLease, LicenseState,
    NeedsOnlineReason, SecureLicenseRecord, SecureStore, ValidationCode,
};

const CLOCK_ROLLBACK_TOLERANCE: i64 = 5 * 60;
const ONLINE_REFRESH_INTERVAL: Duration = Duration::from_secs(15 * 60);
const TRUSTED_TIME_CHECKPOINT_INTERVAL_SECONDS: i64 = 60;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum LicenseOperation {
    #[default]
    Idle,
    Activating,
    Refreshing,
    Deactivating,
}

#[derive(Clone, Debug)]
pub struct LicenseManagerSnapshot {
    pub state: LicenseState,
    pub access: AccessDecision,
    pub operation: LicenseOperation,
    pub configuration_ready: bool,
    pub dev_mode: bool,
    pub masked_key: Option<String>,
    pub message: String,
}

pub struct LicenseManager {
    gate: LicenseGate,
    config: Option<KeygenConfig>,
    store: Option<SecureStore>,
    operation: LicenseOperation,
    receiver: Option<Receiver<WorkerResult>>,
    masked_key: Option<String>,
    message: String,
    next_refresh: Instant,
}

impl Default for LicenseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl LicenseManager {
    /// True when the binary was compiled with `HEAT3_DEV_LICENSE` set.
    /// In that mode licensing is simulated: every feature is unlocked and no
    /// Keygen server is contacted. Never enabled for production builds.
    pub fn development_mode() -> bool {
        option_env!("HEAT3_DEV_LICENSE").is_some()
    }

    pub fn new() -> Self {
        if Self::development_mode() {
            return Self::development();
        }
        let gate = LicenseGate::default();
        let config = KeygenConfig::from_compile_time().ok();
        let store = SecureStore::for_current_user().ok();
        let mut manager = Self {
            gate,
            config,
            store,
            operation: LicenseOperation::Idle,
            receiver: None,
            masked_key: None,
            message: String::new(),
            next_refresh: Instant::now(),
        };
        manager.restore_cached_license();
        manager
    }

    fn development() -> Self {
        let now = unix_now().unwrap_or(0);
        let lease = LicenseLease::new(
            "dev-license",
            "dev-machine",
            now,
            now.saturating_add(10 * 365 * 24 * 60 * 60),
        )
        .expect("development lease range is always valid");
        Self {
            gate: LicenseGate::new(LicenseState::OnlineValid(lease)),
            config: None,
            store: None,
            operation: LicenseOperation::Idle,
            receiver: None,
            masked_key: None,
            message: "Лицензирование отключено переменной HEAT3_DEV_LICENSE; все функции доступны."
                .to_owned(),
            next_refresh: Instant::now(),
        }
    }

    pub fn gate(&self) -> LicenseGate {
        self.gate.clone()
    }

    pub fn snapshot(&self) -> LicenseManagerSnapshot {
        LicenseManagerSnapshot {
            state: self.gate.snapshot().state,
            access: self.gate.authorize_now(),
            operation: self.operation,
            configuration_ready: self.config.is_some() && self.store.is_some(),
            dev_mode: Self::development_mode(),
            masked_key: self.masked_key.clone(),
            message: self.message.clone(),
        }
    }

    pub(crate) fn can_retry_online_refresh(&self) -> bool {
        self.operation == LicenseOperation::Idle
            && self.config.is_some()
            && self.store.is_some()
            && self.masked_key.is_some()
            && is_online_recovery_state(&self.gate.snapshot().state)
    }

    pub fn activate(&mut self, mut license_key: String) -> bool {
        if self.operation != LicenseOperation::Idle {
            license_key.zeroize();
            return false;
        }
        let key = license_key.trim().to_owned();
        license_key.zeroize();
        if key.len() < 8 || key.len() > 7_000 || key.contains(['\r', '\n', '\0']) {
            self.message = "Check the license key format.".to_owned();
            return false;
        }
        let (Some(config), Some(store)) = (self.config.clone(), self.store.clone()) else {
            self.message = configuration_message();
            return false;
        };

        self.operation = LicenseOperation::Activating;
        self.gate.transition_to(LicenseState::Activating);
        self.message = "Checking the key and binding the license to this computer...".to_owned();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        thread::spawn(move || {
            let result = activate_worker(config, store, key);
            let _ = sender.send(result);
        });
        true
    }

    pub fn refresh(&mut self) -> bool {
        if self.operation != LicenseOperation::Idle {
            return false;
        }
        let (Some(config), Some(store)) = (self.config.clone(), self.store.clone()) else {
            self.message = configuration_message();
            return false;
        };
        self.operation = LicenseOperation::Refreshing;
        self.message = "Refreshing license state...".to_owned();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        thread::spawn(move || {
            let result = refresh_worker(config, store);
            let _ = sender.send(result);
        });
        true
    }

    pub fn deactivate(&mut self) -> bool {
        if self.operation != LicenseOperation::Idle {
            return false;
        }
        let (Some(config), Some(store)) = (self.config.clone(), self.store.clone()) else {
            self.message = configuration_message();
            return false;
        };
        if store
            .update(|current| {
                let Some(mut record) = current else {
                    return Err(super::StorageError::InvalidRecord);
                };
                record.set_deactivation_pending(true);
                Ok((Some(record), ()))
            })
            .is_err()
        {
            self.message =
                "Failed to safely mark the local license before deactivation.".to_owned();
            self.gate.transition_to(LicenseState::Tampered);
            return false;
        }
        self.operation = LicenseOperation::Deactivating;
        self.gate.transition_to(LicenseState::NeedsOnline(
            NeedsOnlineReason::RefreshRequired,
        ));
        self.message = "Releasing the server-side activation...".to_owned();
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        thread::spawn(move || {
            let result = deactivate_worker(config, store);
            let _ = sender.send(result);
        });
        true
    }

    /// Poll from the UI frame; never waits for the network worker.
    pub fn poll(&mut self) -> bool {
        if Self::development_mode() {
            return false;
        }
        let result = self.receiver.as_ref().map(Receiver::try_recv);
        if let Some(Ok(result)) = result {
            self.receiver = None;
            self.operation = LicenseOperation::Idle;
            self.masked_key = result.masked_key;
            self.message = result.message;
            self.gate.transition_to(result.state);
            self.next_refresh = Instant::now() + ONLINE_REFRESH_INTERVAL;
            return true;
        }
        if matches!(result, Some(Err(TryRecvError::Disconnected))) {
            let failed_operation = self.operation;
            self.receiver = None;
            self.operation = LicenseOperation::Idle;
            self.message =
                "Background license check ended unexpectedly. Retry the operation.".to_owned();
            if failed_operation == LicenseOperation::Activating {
                self.gate.transition_to(LicenseState::NeedsOnline(
                    NeedsOnlineReason::RefreshRequired,
                ));
            }
            self.next_refresh = Instant::now() + Duration::from_secs(60);
            return true;
        }
        if self.operation == LicenseOperation::Idle {
            let decision = self.gate.authorize_now();
            if decision.is_allowed() {
                if let Some(observed) = self
                    .gate
                    .trusted_time_checkpoint_due(TRUSTED_TIME_CHECKPOINT_INTERVAL_SECONDS)
                {
                    if self.persist_trusted_time(observed).is_err() {
                        self.message =
                            "Failed to safely update the local license record.".to_owned();
                        self.gate.transition_to(LicenseState::Tampered);
                        return true;
                    }
                }
            }
        }
        if self.operation == LicenseOperation::Idle
            && Instant::now() >= self.next_refresh
            && (self.gate.authorize_now().is_allowed() || self.can_retry_online_refresh())
        {
            self.next_refresh = Instant::now() + ONLINE_REFRESH_INTERVAL;
            return self.refresh();
        }
        false
    }

    fn restore_cached_license(&mut self) {
        let (Some(config), Some(store)) = (&self.config, &self.store) else {
            self.message = configuration_message();
            return;
        };
        let record = match store.load() {
            Ok(Some(record)) => record,
            Ok(None) => {
                self.message = "Enter the issued license key.".to_owned();
                return;
            }
            Err(_) => {
                self.message = "Local license data is corrupted or unavailable.".to_owned();
                self.gate.transition_to(LicenseState::Tampered);
                return;
            }
        };
        if record.deactivation_pending() {
            self.masked_key = Some(mask_license_key(record.license_key()));
            self.gate
                .restore_trusted_time(record.max_observed_trusted_time());
            self.message = "Pending deactivation requires online confirmation.".to_owned();
            self.gate.transition_to(LicenseState::NeedsOnline(
                NeedsOnlineReason::RefreshRequired,
            ));
            return;
        }
        if let Some(state) = persisted_authoritative_state(record.authoritative_block()) {
            self.masked_key = Some(mask_license_key(record.license_key()));
            self.gate
                .restore_trusted_time(record.max_observed_trusted_time());
            self.message = persisted_authoritative_message(&state).to_owned();
            self.gate.transition_to(state);
            return;
        }
        let now = unix_now().unwrap_or(i64::MIN);
        let reconciled =
            match store.update(|current| reconcile_restored_record(current, &record, now)) {
                Ok(record) => record,
                Err(_) => {
                    self.message = "Failed to safely update the local license record.".to_owned();
                    self.gate.transition_to(LicenseState::Tampered);
                    return;
                }
            };
        self.masked_key = Some(mask_license_key(reconciled.license_key()));
        self.gate
            .restore_trusted_time(reconciled.max_observed_trusted_time());
        if let Some((state, message)) = blocking_state_from_record(&reconciled) {
            self.message = message;
            self.gate.transition_to(state);
            return;
        }
        if now + CLOCK_ROLLBACK_TOLERANCE < reconciled.max_observed_trusted_time() {
            self.message = "System clock rollback detected. Connect to the internet.".to_owned();
            self.gate
                .transition_to(LicenseState::NeedsOnline(NeedsOnlineReason::ClockRollback));
            return;
        }
        if now >= reconciled.offline_valid_until() {
            self.message = "Offline lease expired. Connect to the internet.".to_owned();
            self.gate
                .transition_to(LicenseState::NeedsOnline(NeedsOnlineReason::LeaseExpired));
            return;
        }
        let hardware = match collect_hardware_identity() {
            Ok(hardware) => hardware,
            Err(_) => {
                self.message = "Could not verify this computer hardware.".to_owned();
                self.gate.transition_to(LicenseState::HardwareMismatch);
                return;
            }
        };
        if !matches_stored_hardware(
            reconciled.fingerprint_schema_version(),
            reconciled.activation_fingerprint(),
            reconciled.hashed_components(),
            &hardware,
        ) {
            self.message = "Current hardware does not match the activated machine.".to_owned();
            self.gate.transition_to(LicenseState::HardwareMismatch);
            return;
        }
        let client = match KeygenClient::new(config.clone()) {
            Ok(client) => client,
            Err(_) => {
                self.message = configuration_message();
                return;
            }
        };
        let verified = match client.verify_cached_machine(&reconciled, now) {
            Ok(verified) => verified,
            Err(_) => {
                self.message =
                    "The cached license signature or contents failed verification.".to_owned();
                self.gate.transition_to(LicenseState::Tampered);
                return;
            }
        };
        let lease = match LicenseLease::new(
            reconciled.license_id(),
            reconciled.machine_id(),
            reconciled.last_successful_online_server_time(),
            verified.expires_at,
        ) {
            Ok(lease) => lease,
            Err(_) => {
                self.gate.transition_to(LicenseState::Tampered);
                return;
            }
        };
        if let Some((state, message)) = blocking_state_from_record(&reconciled) {
            self.message = message;
            self.gate.transition_to(state);
            return;
        }
        self.gate.mark_trusted_time_persisted(now);
        self.message =
            "License verified locally. Internet access is not currently required.".to_owned();
        self.gate.transition_to(LicenseState::OfflineLease(lease));
    }

    fn persist_trusted_time(&self, observed: i64) -> Result<(), ()> {
        let store = self.store.as_ref().ok_or(())?;
        store
            .update(|current| {
                let Some(mut record) = current else {
                    return Err(super::StorageError::InvalidRecord);
                };
                if observed > record.max_observed_trusted_time() {
                    record.observe_time(observed);
                }
                Ok((Some(record), ()))
            })
            .map_err(|_| ())?;
        self.gate.mark_trusted_time_persisted(observed);
        Ok(())
    }
}

impl Drop for LicenseManager {
    fn drop(&mut self) {
        if self.operation == LicenseOperation::Idle && self.gate.authorize_now().is_allowed() {
            let observed = self.gate.observed_trusted_time();
            let _ = self.persist_trusted_time(observed);
        }
    }
}

struct WorkerResult {
    state: LicenseState,
    masked_key: Option<String>,
    message: String,
}

fn activate_worker(config: KeygenConfig, store: SecureStore, mut key: String) -> WorkerResult {
    let masked = mask_license_key(&key);
    let baseline = match store.load() {
        Ok(current) => current,
        Err(_) => return failure_result(WorkerFailure::Storage, Some(masked)),
    };
    let result = (|| -> Result<WorkerResult, WorkerFailure> {
        let hardware = collect_hardware_identity().map_err(|_| WorkerFailure::Hardware)?;
        let client = KeygenClient::new(config).map_err(WorkerFailure::Client)?;
        let first = client
            .validate_key(&key, &hardware)
            .map_err(WorkerFailure::Client)?;
        let license_id = first
            .license_id
            .clone()
            .ok_or_else(|| WorkerFailure::Validation(first.code.clone()))?;
        if !matches!(
            first.code,
            ValidationCode::Valid | ValidationCode::NoMachine
        ) {
            return Err(WorkerFailure::Validation(first.code));
        }
        let machine = match client
            .find_machine(&key, &license_id, &hardware.fingerprint)
            .map_err(WorkerFailure::Client)?
        {
            Some(machine) => machine,
            None => client
                .activate_machine(&key, &license_id, &hardware, &computer_name())
                .map_err(WorkerFailure::Client)?,
        };
        let validation = client
            .validate_key(&key, &hardware)
            .map_err(WorkerFailure::Client)?;
        if !validation.valid {
            return Err(WorkerFailure::Validation(validation.code));
        }
        let check_in_time = client
            .check_in(&key, &license_id)
            .map_err(WorkerFailure::Client)?;
        let checkout = client
            .checkout_machine(
                &key,
                &license_id,
                &machine.machine_id,
                &hardware.fingerprint,
            )
            .map_err(WorkerFailure::Client)?;
        let last_online = first
            .server_time
            .max(machine.server_time)
            .max(validation.server_time)
            .max(check_in_time)
            .max(checkout.server_time);
        let lease = LicenseLease::new(
            &license_id,
            &machine.machine_id,
            last_online,
            checkout.verified.expires_at,
        )
        .map_err(|_| WorkerFailure::Corrupt)?;
        let record = SecureLicenseRecord::new(
            &key,
            &license_id,
            &machine.machine_id,
            &hardware.fingerprint,
            &hardware.schema,
            hardware.components,
            checkout.certificate,
            last_online,
            last_online,
            checkout.verified.expires_at,
        )
        .map_err(|_| WorkerFailure::Storage)?;
        match store
            .update(|current| replace_record_if_unchanged(current, baseline.as_ref(), &record))
        {
            Ok(()) => Ok(WorkerResult {
                state: LicenseState::OnlineValid(lease),
                masked_key: Some(masked.clone()),
                message: "License activated and bound to this computer.".to_owned(),
            }),
            Err(super::StorageError::ConcurrentModification) => Ok(conflict_worker_result(
                &store,
                "Local license was changed by another operation. Kept the current state.",
            )),
            Err(_) => Err(WorkerFailure::Storage),
        }
    })();
    key.zeroize();
    match result {
        Ok(worker_result) => worker_result,
        Err(error) => failure_result(error, Some(masked)),
    }
}

fn refresh_worker(config: KeygenConfig, store: SecureStore) -> WorkerResult {
    let record = match store.load() {
        Ok(Some(record)) => record,
        Ok(None) => return failure_result(WorkerFailure::Missing, None),
        Err(_) => return failure_result(WorkerFailure::Storage, None),
    };
    if record.deactivation_pending() {
        return deactivate_worker(config, store);
    }
    let masked = Some(mask_license_key(record.license_key()));
    let result = (|| {
        let hardware = collect_hardware_identity().map_err(|_| WorkerFailure::Hardware)?;
        if !matches_stored_hardware(
            record.fingerprint_schema_version(),
            record.activation_fingerprint(),
            record.hashed_components(),
            &hardware,
        ) {
            return Err(WorkerFailure::Hardware);
        }
        let client = KeygenClient::new(config).map_err(WorkerFailure::Client)?;
        let validation = client
            .validate_key(record.license_key(), &hardware)
            .map_err(WorkerFailure::Client)?;
        if !validation.valid || validation.license_id.as_deref() != Some(record.license_id()) {
            let persisted = persist_authoritative_validation(&store, &record, &validation.code)
                .map_err(|_| WorkerFailure::Storage)?;
            if same_record_binding(&persisted, &record) {
                return Err(WorkerFailure::Validation(validation.code));
            }
            return Ok(local_record_result(
                &persisted,
                "Local license was changed by another operation. Kept the current state.",
            ));
        }
        let check_in = client
            .check_in(record.license_key(), record.license_id())
            .map_err(WorkerFailure::Client)?;
        let checkout = client
            .checkout_machine(
                record.license_key(),
                record.license_id(),
                record.machine_id(),
                record.activation_fingerprint(),
            )
            .map_err(WorkerFailure::Client)?;
        let last_online = validation
            .server_time
            .max(check_in)
            .max(checkout.server_time);
        let updated = SecureLicenseRecord::new(
            record.license_key(),
            record.license_id(),
            record.machine_id(),
            record.activation_fingerprint(),
            record.fingerprint_schema_version(),
            record.hashed_components().to_vec(),
            checkout.certificate,
            last_online,
            last_online,
            checkout.verified.expires_at,
        )
        .map_err(|_| WorkerFailure::Storage)?;
        let committed = store
            .update(|current| {
                let merged = merge_refreshed_record(current, &record, updated);
                Ok((Some(merged.clone()), merged))
            })
            .map_err(|_| WorkerFailure::Storage)?;
        if let Some((state, message)) = blocking_state_from_record(&committed) {
            return Ok(WorkerResult {
                state,
                masked_key: masked.clone(),
                message,
            });
        }
        let committed_lease =
            lease_from_record_at(&committed, committed.last_successful_online_server_time())
                .ok_or(WorkerFailure::Storage)?;
        Ok(WorkerResult {
            state: LicenseState::OnlineValid(committed_lease),
            masked_key: masked.clone(),
            message: "License refreshed. New offline lease saved.".to_owned(),
        })
    })();
    match result {
        Ok(worker_result) => worker_result,
        Err(error) if matches!(&error, WorkerFailure::Client(client_error) if is_transient(client_error)) => {
            match store.load() {
                Ok(Some(current)) if same_record_binding(&current, &record) => {
                    if let Some((state, _message)) = blocking_state_from_record(&current) {
                        WorkerResult {
                            state,
                            masked_key: Some(mask_license_key(current.license_key())),
                            message: "Licensing service unavailable. Continuing within the signed offline lease.".to_owned(),
                        }
                    } else if let Some(lease) = lease_from_record(&current) {
                        WorkerResult {
                            state: LicenseState::ServiceUnavailable(lease),
                            masked_key: Some(mask_license_key(current.license_key())),
                            message: "Licensing service unavailable. Continuing within the signed offline lease.".to_owned(),
                        }
                    } else {
                        failure_result(error, masked)
                    }
                }
                Ok(Some(current)) => local_record_result(
                    &current,
                    "Local license was changed by another operation. Kept the current state.",
                ),
                Ok(None) => failure_result(WorkerFailure::Missing, None),
                Err(_) => failure_result(WorkerFailure::Storage, None),
            }
        }

        Err(error) => failure_result(error, masked),
    }
}

fn deactivate_worker(config: KeygenConfig, store: SecureStore) -> WorkerResult {
    let record = match store.load() {
        Ok(Some(record)) => record,
        Ok(None) => return failure_result(WorkerFailure::Missing, None),
        Err(_) => return failure_result(WorkerFailure::Storage, None),
    };
    let result = KeygenClient::new(config)
        .and_then(|client| client.deactivate_machine(record.license_key(), record.machine_id()));
    match result {
        Ok(_) => deactivation_success_result(&store, &record),
        Err(error) if deactivation_already_completed(&error) => {
            deactivation_success_result(&store, &record)
        }
        Err(_error) => match clear_deactivation_pending(&store, &record) {
            Ok(Some(current)) => local_record_result(
                &current,
                "Deactivation was not confirmed by the server. Kept the current local license state.",
            ),
            Ok(None) => failure_result(WorkerFailure::Missing, None),
            Err(_) => failure_result(
                WorkerFailure::Storage,
                Some(mask_license_key(record.license_key())),
            ),
        },




    }
}

fn deactivation_success_result(store: &SecureStore, record: &SecureLicenseRecord) -> WorkerResult {
    match store.update(|current| match current {
        Some(current) if same_record_binding(&current, record) => Ok((None, None)),
        Some(current) => {
            let preserved = current.clone();
            Ok((Some(current), Some(preserved)))
        }
        None => Ok((None, None)),
    }) {
        Ok(None) => WorkerResult {
            state: LicenseState::Unlicensed,
            masked_key: None,
            message: "License deactivated. The server-side activation was released.".to_owned(),
        },
        Ok(Some(current)) => local_record_result(
            &current,
            "Local license was changed by another operation. Kept the current state.",
        ),
        Err(_) => WorkerResult {
            state: LicenseState::NeedsOnline(NeedsOnlineReason::RefreshRequired),
            masked_key: Some(mask_license_key(record.license_key())),
            message: "Server confirmed deactivation, but the local license was not removed. Online recovery is required.".to_owned(),
        },
    }
}

fn merge_refreshed_record(
    current: Option<SecureLicenseRecord>,
    original: &SecureLicenseRecord,
    mut candidate: SecureLicenseRecord,
) -> SecureLicenseRecord {
    candidate.set_authoritative_block(None);
    candidate.set_deactivation_pending(false);
    if let Some(mut current) = current {
        if should_preserve_current_record(&current, original) {
            current.observe_time(candidate.max_observed_trusted_time());
            return current;
        }
        candidate.observe_time(current.max_observed_trusted_time());
    }
    candidate
}

fn clear_deactivation_pending(
    store: &SecureStore,
    original: &SecureLicenseRecord,
) -> Result<Option<SecureLicenseRecord>, super::StorageError> {
    store.update(|current| match current {
        Some(mut record) if same_record_binding(&record, original) => {
            record.set_deactivation_pending(false);
            let persisted = record.clone();
            Ok((Some(record), Some(persisted)))
        }
        Some(record) => {
            let preserved = record.clone();
            Ok((Some(record), Some(preserved)))
        }
        None => Ok((None, None)),
    })
}

fn persist_authoritative_validation(
    store: &SecureStore,
    original: &SecureLicenseRecord,
    code: &ValidationCode,
) -> Result<SecureLicenseRecord, super::StorageError> {
    let persisted = authoritative_code(code);
    store.update(|current| {
        let Some(mut record) = current else {
            return Err(super::StorageError::InvalidRecord);
        };
        if !same_record_binding(&record, original) {
            let preserved = record.clone();
            return Ok((Some(record), preserved));
        }
        record.set_authoritative_block(Some(persisted.to_owned()));
        record.set_deactivation_pending(false);
        let persisted = record.clone();
        Ok((Some(record), persisted))
    })
}

fn authoritative_code(code: &ValidationCode) -> &'static str {
    match code {
        ValidationCode::Suspended => "suspended",
        ValidationCode::Expired | ValidationCode::Overdue => "expired",
        ValidationCode::TooManyMachines => "too_many_machines",
        _ => "unlicensed",
    }
}

fn deactivation_already_completed(error: &KeygenClientError) -> bool {
    matches!(error, KeygenClientError::Api { status: 404, .. })
}

fn persisted_authoritative_state(code: Option<&str>) -> Option<LicenseState> {
    match code {
        Some("suspended") => Some(LicenseState::Suspended),
        Some("expired") => Some(LicenseState::Expired),
        Some("too_many_machines") | Some("unlicensed") => Some(LicenseState::Unlicensed),
        Some(_) => Some(LicenseState::NeedsOnline(
            NeedsOnlineReason::RefreshRequired,
        )),
        None => None,
    }
}

fn persisted_authoritative_message(state: &LicenseState) -> &'static str {
    match state {
        LicenseState::Suspended => "License suspended by the owner. Contact support.",
        LicenseState::Expired => "License term expired.",
        LicenseState::Unlicensed => {
            "Local license was authoritatively rejected by the server and must be activated again."
        }
        LicenseState::NeedsOnline(_) => "Saved license state requires online confirmation.",
        _ => "Saved license state is unavailable.",
    }
}

fn is_online_recovery_state(state: &LicenseState) -> bool {
    matches!(
        state,
        LicenseState::NeedsOnline(_)
            | LicenseState::Suspended
            | LicenseState::Expired
            | LicenseState::Unlicensed
    )
}

fn blocking_state_from_record(record: &SecureLicenseRecord) -> Option<(LicenseState, String)> {
    if record.deactivation_pending() {
        return Some((
            LicenseState::NeedsOnline(NeedsOnlineReason::RefreshRequired),
            "Pending deactivation requires online confirmation.".to_owned(),
        ));
    }
    persisted_authoritative_state(record.authoritative_block()).map(|state| {
        let message = persisted_authoritative_message(&state).to_owned();
        (state, message)
    })
}

fn local_record_result(record: &SecureLicenseRecord, message: &str) -> WorkerResult {
    if let Some((state, blocked_message)) = blocking_state_from_record(record) {
        return WorkerResult {
            state,
            masked_key: Some(mask_license_key(record.license_key())),
            message: blocked_message,
        };
    }
    if let Some(lease) = lease_from_record(record) {
        return WorkerResult {
            state: LicenseState::OfflineLease(lease),
            masked_key: Some(mask_license_key(record.license_key())),
            message: message.to_owned(),
        };
    }
    failure_result(
        WorkerFailure::Storage,
        Some(mask_license_key(record.license_key())),
    )
}

fn conflict_worker_result(store: &SecureStore, message: &str) -> WorkerResult {
    match store.load() {
        Ok(Some(record)) => local_record_result(&record, message),
        Ok(None) => failure_result(WorkerFailure::Missing, None),
        Err(_) => failure_result(WorkerFailure::Storage, None),
    }
}

fn replace_record_if_unchanged(
    current: Option<SecureLicenseRecord>,
    expected: Option<&SecureLicenseRecord>,
    next: &SecureLicenseRecord,
) -> Result<(Option<SecureLicenseRecord>, ()), super::StorageError> {
    if current.as_ref() != expected {
        return Err(super::StorageError::ConcurrentModification);
    }
    Ok((Some(next.clone()), ()))
}

fn reconcile_restored_record(
    current: Option<SecureLicenseRecord>,
    original: &SecureLicenseRecord,
    now: i64,
) -> Result<(Option<SecureLicenseRecord>, SecureLicenseRecord), super::StorageError> {
    let Some(mut current) = current else {
        return Err(super::StorageError::InvalidRecord);
    };
    if should_preserve_current_record(&current, original) {
        let preserved = current.clone();
        return Ok((Some(current), preserved));
    }
    current.observe_time(now);
    current.set_authoritative_block(None);
    current.set_deactivation_pending(false);
    let persisted = current.clone();
    Ok((Some(persisted), current))
}

fn same_record_binding(current: &SecureLicenseRecord, original: &SecureLicenseRecord) -> bool {
    current.license_id() == original.license_id()
        && current.machine_id() == original.machine_id()
        && current.activation_fingerprint() == original.activation_fingerprint()
        && current.fingerprint_schema_version() == original.fingerprint_schema_version()
        && current.hashed_components() == original.hashed_components()
}

fn should_preserve_current_record(
    current: &SecureLicenseRecord,
    original: &SecureLicenseRecord,
) -> bool {
    if !same_record_binding(current, original) {
        return true;
    }
    if current.authoritative_block().is_some()
        && current.authoritative_block() != original.authoritative_block()
    {
        return true;
    }
    if current.deactivation_pending() && !original.deactivation_pending() {
        return true;
    }
    current.last_successful_online_server_time() > original.last_successful_online_server_time()
        || current.offline_valid_until() > original.offline_valid_until()
}

enum WorkerFailure {
    Client(KeygenClientError),
    Validation(ValidationCode),
    Hardware,
    Storage,
    Missing,
    Corrupt,
}

fn failure_result(error: WorkerFailure, masked_key: Option<String>) -> WorkerResult {
    let (state, message) = match error {
        WorkerFailure::Validation(ValidationCode::Suspended) => (
            LicenseState::Suspended,
            "License suspended by the owner. Contact support.",
        ),
        WorkerFailure::Validation(ValidationCode::Expired | ValidationCode::Overdue) => {
            (LicenseState::Expired, "License term expired.")
        }
        WorkerFailure::Validation(ValidationCode::TooManyMachines) => {
            (LicenseState::Unlicensed, "Activated machine limit reached.")
        }
        WorkerFailure::Validation(_) => (
            LicenseState::Unlicensed,
            "The key did not validate for this product or computer.",
        ),
        WorkerFailure::Hardware => (
            LicenseState::HardwareMismatch,
            "Could not verify the hardware binding.",
        ),
        WorkerFailure::Client(KeygenClientError::Timeout | KeygenClientError::Connection) => (
            LicenseState::NeedsOnline(NeedsOnlineReason::RefreshRequired),
            "Licensing service unavailable. Check the internet connection and retry.",
        ),
        WorkerFailure::Client(_) => (
            LicenseState::NeedsOnline(NeedsOnlineReason::RefreshRequired),
            "Protected license check failed.",
        ),
        WorkerFailure::Storage => (
            LicenseState::Tampered,
            "Failed to safely store license data.",
        ),
        WorkerFailure::Missing => (LicenseState::Unlicensed, "Saved license was not found."),
        WorkerFailure::Corrupt => (
            LicenseState::Tampered,
            "Server returned inconsistent license data.",
        ),
    };
    WorkerResult {
        state,
        masked_key,
        message: message.to_owned(),
    }
}

fn unix_now() -> Option<i64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|duration| i64::try_from(duration.as_secs()).ok())
}

fn lease_from_record(record: &SecureLicenseRecord) -> Option<LicenseLease> {
    lease_from_record_at(record, unix_now()?)
}

fn lease_from_record_at(record: &SecureLicenseRecord, now: i64) -> Option<LicenseLease> {
    let observed = record.max_observed_trusted_time();
    if now.saturating_add(CLOCK_ROLLBACK_TOLERANCE) < observed {
        return None;
    }
    let lease = LicenseLease::new(
        record.license_id(),
        record.machine_id(),
        record.last_successful_online_server_time(),
        record.offline_valid_until(),
    )
    .ok()?;
    lease.is_valid_at(now.max(observed)).then_some(lease)
}

fn is_transient(error: &KeygenClientError) -> bool {
    matches!(
        error,
        KeygenClientError::Timeout
            | KeygenClientError::Connection
            | KeygenClientError::Api { status: 429, .. }
            | KeygenClientError::Api {
                status: 500..=599,
                ..
            }
    )
}

fn computer_name() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Windows PC".to_owned())
}

fn configuration_message() -> String {
    "This build is not configured for licensing. Keygen CE settings are required.".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::licensing::{
        AccessMode, HardwareComponent, HardwareComponentKind, HARDWARE_SCHEMA_VERSION,
    };
    use base64::{engine::general_purpose::STANDARD, Engine as _};
    use ed25519_dalek::SigningKey;

    fn test_config() -> KeygenConfig {
        let public_key = STANDARD.encode(
            SigningKey::from_bytes(&[4u8; 32])
                .verifying_key()
                .as_bytes(),
        );
        KeygenConfig::new(
            "https://licensing.example.test",
            "account-1",
            "product-1",
            "policy-1",
            public_key,
            7 * 24 * 60 * 60,
        )
        .unwrap()
    }

    #[test]
    fn server_outage_is_transient_but_client_error_is_not() {
        assert!(is_transient(&KeygenClientError::Api {
            status: 503,
            code: None,
        }));
        assert!(is_transient(&KeygenClientError::Api {
            status: 429,
            code: None,
        }));
        assert!(!is_transient(&KeygenClientError::Api {
            status: 403,
            code: None,
        }));
    }

    #[test]
    fn disconnected_activation_worker_cannot_leave_manager_busy() {
        let (sender, receiver) = mpsc::channel();
        drop(sender);
        let gate = LicenseGate::new(LicenseState::Activating);
        let mut manager = LicenseManager {
            gate: gate.clone(),
            config: None,
            store: None,
            operation: LicenseOperation::Activating,
            receiver: Some(receiver),
            masked_key: None,
            message: String::new(),
            next_refresh: Instant::now(),
        };

        assert!(manager.poll());
        assert_eq!(manager.operation, LicenseOperation::Idle);
        assert!(matches!(
            gate.snapshot().state,
            LicenseState::NeedsOnline(NeedsOnlineReason::RefreshRequired)
        ));
    }

    #[test]
    fn lease_from_record_keeps_signed_online_time_as_lower_bound() {
        let record = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            10,
            25,
            40,
        )
        .unwrap();

        let lease = lease_from_record_at(&record, 30).unwrap();
        assert_eq!(lease.last_online_at(), 10);
        assert_eq!(lease.offline_valid_until(), 40);
    }

    #[test]
    fn lease_from_record_applies_clock_rollback_tolerance() {
        let record = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            1_000,
            1_100,
            2_000,
        )
        .unwrap();

        let lease = lease_from_record_at(&record, 900).unwrap();
        assert_eq!(lease.last_online_at(), 1_000);
        assert_eq!(lease.offline_valid_until(), 2_000);
    }

    fn manager_with_persisted_record(
        test_name: &str,
    ) -> (LicenseManager, LicenseGate, SecureStore, i64) {
        let now = unix_now().unwrap();
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let store = SecureStore::at(std::env::temp_dir().join(format!(
            "heat3-license-manager-{test_name}-{}-{unique}.bin",
            std::process::id()
        )));
        let record = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            now - 10,
            now - 10,
            now + 600,
        )
        .unwrap();
        store.save(&record).unwrap();
        let gate = LicenseGate::new(LicenseState::OfflineLease(
            LicenseLease::new("license-id", "machine-id", now - 10, now + 600).unwrap(),
        ));
        assert!(gate.authorize_at_trusted(now).is_allowed());
        let manager = LicenseManager {
            gate: gate.clone(),
            config: None,
            store: Some(store.clone()),
            operation: LicenseOperation::Idle,
            receiver: None,
            masked_key: None,
            message: String::new(),
            next_refresh: Instant::now() + Duration::from_secs(60),
        };
        (manager, gate, store, now)
    }

    #[test]
    fn restore_cached_license_persists_authoritative_suspend_across_restart() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let store = SecureStore::at(std::env::temp_dir().join(format!(
            "heat3-license-manager-suspend-{}-{unique}.bin",
            std::process::id()
        )));
        let mut record = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            10,
            10,
            unix_now().unwrap() + 600,
        )
        .unwrap();
        record.set_authoritative_block(Some("suspended".to_owned()));
        store.save(&record).unwrap();

        let gate = LicenseGate::default();
        let mut manager = LicenseManager {
            gate: gate.clone(),
            config: Some(test_config()),
            store: Some(store.clone()),
            operation: LicenseOperation::Idle,
            receiver: None,
            masked_key: None,
            message: String::new(),
            next_refresh: Instant::now(),
        };

        manager.restore_cached_license();

        assert!(matches!(gate.snapshot().state, LicenseState::Suspended));
        store.delete().unwrap();
    }

    #[test]
    fn restore_cached_license_blocks_when_deactivation_is_pending() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let store = SecureStore::at(std::env::temp_dir().join(format!(
            "heat3-license-manager-pending-{}-{unique}.bin",
            std::process::id()
        )));
        let mut record = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            10,
            10,
            unix_now().unwrap() + 600,
        )
        .unwrap();
        record.set_deactivation_pending(true);
        store.save(&record).unwrap();

        let gate = LicenseGate::default();
        let mut manager = LicenseManager {
            gate: gate.clone(),
            config: Some(test_config()),
            store: Some(store.clone()),
            operation: LicenseOperation::Idle,
            receiver: None,
            masked_key: None,
            message: String::new(),
            next_refresh: Instant::now(),
        };

        manager.restore_cached_license();

        assert!(matches!(
            gate.snapshot().state,
            LicenseState::NeedsOnline(NeedsOnlineReason::RefreshRequired)
        ));
        store.delete().unwrap();
    }

    #[test]
    fn blocked_recovery_state_can_retry_online_refresh() {
        let manager = LicenseManager {
            gate: LicenseGate::new(LicenseState::NeedsOnline(
                NeedsOnlineReason::RefreshRequired,
            )),
            config: Some(test_config()),
            store: Some(SecureStore::at(
                std::env::temp_dir().join("recovery-license.bin"),
            )),
            operation: LicenseOperation::Idle,
            receiver: None,
            masked_key: Some("****-1234".to_owned()),
            message: String::new(),
            next_refresh: Instant::now(),
        };

        assert!(manager.can_retry_online_refresh());
    }

    #[test]
    fn merge_refreshed_record_preserves_newer_authoritative_block() {
        let mut current = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            20,
            20,
            620,
        )
        .unwrap();
        current.set_authoritative_block(Some("suspended".to_owned()));
        let original = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            10,
            10,
            610,
        )
        .unwrap();
        let updated = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            30,
            30,
            630,
        )
        .unwrap();

        let merged = merge_refreshed_record(Some(current), &original, updated);

        assert_eq!(merged.authoritative_block(), Some("suspended"));
    }

    #[test]
    fn merge_refreshed_record_preserves_newer_deactivation_pending() {
        let mut current = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            20,
            20,
            620,
        )
        .unwrap();
        current.set_deactivation_pending(true);
        let original = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            10,
            10,
            610,
        )
        .unwrap();
        let updated = SecureLicenseRecord::new(
            "key",
            "license-id",
            "machine-id",
            "fingerprint",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            30,
            30,
            630,
        )
        .unwrap();

        let merged = merge_refreshed_record(Some(current), &original, updated);

        assert!(merged.deactivation_pending());
    }

    #[test]
    fn pending_deactivation_treats_not_found_as_completed() {
        assert!(deactivation_already_completed(&KeygenClientError::Api {
            status: 404,
            code: Some("NOT_FOUND".to_owned()),
        }));
        assert!(!deactivation_already_completed(&KeygenClientError::Api {
            status: 503,
            code: None,
        }));
    }

    #[test]
    fn deactivation_success_preserves_replaced_record() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let store = SecureStore::at(std::env::temp_dir().join(format!(
            "heat3-license-manager-deactivate-replaced-{}-{unique}.bin",
            std::process::id()
        )));
        let original = SecureLicenseRecord::new(
            "old-key",
            "old-license",
            "old-machine",
            "fingerprint-a",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "a".repeat(64),
            }],
            vec![1, 2, 3],
            10,
            10,
            unix_now().unwrap() + 600,
        )
        .unwrap();
        let replacement = SecureLicenseRecord::new(
            "new-key",
            "new-license",
            "new-machine",
            "fingerprint-b",
            HARDWARE_SCHEMA_VERSION,
            vec![HardwareComponent {
                kind: HardwareComponentKind::SystemUuid,
                digest: "b".repeat(64),
            }],
            vec![4, 5, 6],
            20,
            20,
            unix_now().unwrap() + 900,
        )
        .unwrap();
        store.save(&replacement).unwrap();

        let result = deactivation_success_result(&store, &original);
        let persisted = store.load().unwrap().unwrap();

        assert_eq!(persisted.license_id(), "new-license");
        assert!(matches!(result.state, LicenseState::OfflineLease(_)));
        assert_eq!(result.masked_key, Some(mask_license_key("new-key")));

        store.delete().unwrap();
    }

    #[test]
    fn snapshot_access_uses_trusted_gate_decision() {
        let now = unix_now().unwrap();
        let gate = LicenseGate::new(LicenseState::OfflineLease(
            LicenseLease::new("license-id", "machine-id", now - 10, now + 600).unwrap(),
        ));
        gate.restore_trusted_time(now);
        let manager = LicenseManager {
            gate,
            config: None,
            store: None,
            operation: LicenseOperation::Idle,
            receiver: None,
            masked_key: None,
            message: String::new(),
            next_refresh: Instant::now(),
        };

        assert_eq!(
            manager.snapshot().access,
            crate::licensing::AccessDecision::Allowed {
                mode: AccessMode::Offline,
                valid_until: now + 600,
            }
        );
    }

    #[test]
    fn poll_persists_observed_trusted_time() {
        let (mut manager, _gate, store, now) = manager_with_persisted_record("poll");

        manager.poll();

        let record = store.load().unwrap().unwrap();
        assert!(record.max_observed_trusted_time() >= now);
        drop(manager);
        store.delete().unwrap();
    }

    #[test]
    fn drop_persists_observed_trusted_time() {
        let (manager, _gate, store, now) = manager_with_persisted_record("drop");

        drop(manager);

        let record = store.load().unwrap().unwrap();
        assert!(record.max_observed_trusted_time() >= now);
        store.delete().unwrap();
    }
}
