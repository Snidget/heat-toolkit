use std::fmt;

/// Подписанный сервером временной интервал доступа.
///
/// В структуре намеренно нет лицензионного ключа. Ключ хранится только в
/// защищенном storage-слое и не должен попадать в UI snapshot или логи.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LicenseLease {
    license_id: String,
    machine_id: String,
    last_online_at: i64,
    offline_valid_until: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LeaseError {
    MissingLicenseId,
    MissingMachineId,
    InvalidTimeRange,
}

impl fmt::Display for LeaseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::MissingLicenseId => "license id is empty",
            Self::MissingMachineId => "machine id is empty",
            Self::InvalidTimeRange => "offline validity precedes the last online validation",
        };
        f.write_str(message)
    }
}

impl std::error::Error for LeaseError {}

impl LicenseLease {
    pub fn new(
        license_id: impl Into<String>,
        machine_id: impl Into<String>,
        last_online_at: i64,
        offline_valid_until: i64,
    ) -> Result<Self, LeaseError> {
        let license_id = license_id.into();
        let machine_id = machine_id.into();

        if license_id.trim().is_empty() {
            return Err(LeaseError::MissingLicenseId);
        }
        if machine_id.trim().is_empty() {
            return Err(LeaseError::MissingMachineId);
        }
        if offline_valid_until <= last_online_at {
            return Err(LeaseError::InvalidTimeRange);
        }

        Ok(Self {
            license_id,
            machine_id,
            last_online_at,
            offline_valid_until,
        })
    }

    pub fn license_id(&self) -> &str {
        &self.license_id
    }

    pub fn machine_id(&self) -> &str {
        &self.machine_id
    }

    pub fn last_online_at(&self) -> i64 {
        self.last_online_at
    }

    pub fn offline_valid_until(&self) -> i64 {
        self.offline_valid_until
    }

    pub fn is_valid_at(&self, unix_seconds: i64) -> bool {
        unix_seconds >= self.last_online_at && unix_seconds < self.offline_valid_until
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NeedsOnlineReason {
    LeaseExpired,
    ClockRollback,
    MissingTrustedState,
    RefreshRequired,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum LicenseState {
    #[default]
    Unlicensed,
    Activating,
    OnlineValid(LicenseLease),
    OfflineLease(LicenseLease),
    ServiceUnavailable(LicenseLease),
    NeedsOnline(NeedsOnlineReason),
    Suspended,
    Expired,
    HardwareMismatch,
    Tampered,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessMode {
    Online,
    Offline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockReason {
    NotActivated,
    ActivationInProgress,
    OfflineLeaseExpired,
    ClockRollback,
    MissingTrustedState,
    OnlineRefreshRequired,
    Suspended,
    Expired,
    HardwareMismatch,
    Tampered,
    InternalState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessDecision {
    Allowed { mode: AccessMode, valid_until: i64 },
    Denied(BlockReason),
}

impl AccessDecision {
    pub fn is_allowed(self) -> bool {
        matches!(self, Self::Allowed { .. })
    }
}

impl LicenseState {
    pub fn access_at(&self, unix_seconds: i64) -> AccessDecision {
        match self {
            Self::OnlineValid(lease) if lease.is_valid_at(unix_seconds) => {
                AccessDecision::Allowed {
                    mode: AccessMode::Online,
                    valid_until: lease.offline_valid_until(),
                }
            }
            Self::OfflineLease(lease) | Self::ServiceUnavailable(lease)
                if lease.is_valid_at(unix_seconds) =>
            {
                AccessDecision::Allowed {
                    mode: AccessMode::Offline,
                    valid_until: lease.offline_valid_until(),
                }
            }
            Self::OnlineValid(_) | Self::OfflineLease(_) | Self::ServiceUnavailable(_) => {
                AccessDecision::Denied(BlockReason::OfflineLeaseExpired)
            }
            Self::Unlicensed => AccessDecision::Denied(BlockReason::NotActivated),
            Self::Activating => AccessDecision::Denied(BlockReason::ActivationInProgress),
            Self::NeedsOnline(reason) => AccessDecision::Denied(match reason {
                NeedsOnlineReason::LeaseExpired => BlockReason::OfflineLeaseExpired,
                NeedsOnlineReason::ClockRollback => BlockReason::ClockRollback,
                NeedsOnlineReason::MissingTrustedState => BlockReason::MissingTrustedState,
                NeedsOnlineReason::RefreshRequired => BlockReason::OnlineRefreshRequired,
            }),
            Self::Suspended => AccessDecision::Denied(BlockReason::Suspended),
            Self::Expired => AccessDecision::Denied(BlockReason::Expired),
            Self::HardwareMismatch => AccessDecision::Denied(BlockReason::HardwareMismatch),
            Self::Tampered => AccessDecision::Denied(BlockReason::Tampered),
        }
    }
}

/// Возвращает только короткий хвост ключа для безопасного отображения.
pub fn mask_license_key(key: &str) -> String {
    let normalized = key.trim();
    let suffix: String = normalized
        .chars()
        .rev()
        .take(4)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    if suffix.is_empty() {
        "••••".to_owned()
    } else {
        format!("••••-{suffix}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease() -> LicenseLease {
        LicenseLease::new("license-1", "machine-1", 1_000, 2_000).unwrap()
    }

    #[test]
    fn lease_rejects_missing_identity_and_invalid_time_range() {
        assert_eq!(
            LicenseLease::new("", "machine", 1, 2),
            Err(LeaseError::MissingLicenseId)
        );
        assert_eq!(
            LicenseLease::new("license", " ", 1, 2),
            Err(LeaseError::MissingMachineId)
        );
        assert_eq!(
            LicenseLease::new("license", "machine", 2, 2),
            Err(LeaseError::InvalidTimeRange)
        );
    }

    #[test]
    fn lease_is_valid_only_inside_signed_interval() {
        let lease = lease();
        assert!(!lease.is_valid_at(999));
        assert!(lease.is_valid_at(1_000));
        assert!(lease.is_valid_at(1_999));
        assert!(!lease.is_valid_at(2_000));
    }

    #[test]
    fn access_is_fail_closed_when_lease_has_expired() {
        for state in [
            LicenseState::OnlineValid(lease()),
            LicenseState::OfflineLease(lease()),
            LicenseState::ServiceUnavailable(lease()),
        ] {
            assert_eq!(
                state.access_at(2_000),
                AccessDecision::Denied(BlockReason::OfflineLeaseExpired)
            );
        }
    }

    #[test]
    fn mask_exposes_only_last_four_characters() {
        let key = "AAAA-BBBB-CCCC-1234";
        let masked = mask_license_key(key);
        assert_eq!(masked, "••••-1234");
        assert!(!masked.contains("AAAA"));
        assert!(!masked.contains("BBBB"));
        assert!(!masked.contains("CCCC"));
    }
}
