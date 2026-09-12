use std::sync::{
    atomic::{AtomicI64, AtomicU64, Ordering},
    Arc, RwLock,
};
use std::time::{SystemTime, UNIX_EPOCH};

use super::{AccessDecision, BlockReason, LicenseState};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateSnapshot {
    pub revision: u64,
    pub state: LicenseState,
}

#[derive(Clone, Debug)]
pub struct LicenseGate {
    inner: Arc<GateInner>,
}

#[derive(Debug)]
struct GateInner {
    state: RwLock<LicenseState>,
    revision: AtomicU64,
    trusted_floor: AtomicI64,
    persisted_trusted_floor: AtomicI64,
}

const CLOCK_ROLLBACK_TOLERANCE: i64 = 5 * 60;

impl Default for LicenseGate {
    fn default() -> Self {
        Self::new(LicenseState::Unlicensed)
    }
}

impl LicenseGate {
    pub fn new(initial_state: LicenseState) -> Self {
        Self {
            inner: Arc::new(GateInner {
                state: RwLock::new(initial_state),
                revision: AtomicU64::new(0),
                trusted_floor: AtomicI64::new(0),
                persisted_trusted_floor: AtomicI64::new(0),
            }),
        }
    }

    /// Атомарно публикует подтвержденное состояние для UI и рабочих функций.
    pub fn transition_to(&self, next_state: LicenseState) -> u64 {
        let Ok(mut state) = self.inner.state.write() else {
            return self.inner.revision.load(Ordering::SeqCst);
        };

        *state = next_state;
        self.inner.revision.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// Снимок для UI. При повреждении синхронизации возвращает fail-closed state.
    pub fn snapshot(&self) -> GateSnapshot {
        loop {
            let before = self.inner.revision.load(Ordering::SeqCst);
            let state = match self.inner.state.read() {
                Ok(state) => state.clone(),
                Err(_) => LicenseState::Tampered,
            };
            let after = self.inner.revision.load(Ordering::SeqCst);

            if before == after {
                return GateSnapshot {
                    revision: after,
                    state,
                };
            }
        }
    }

    pub fn authorize_at(&self, unix_seconds: i64) -> AccessDecision {
        self.snapshot().state.access_at(unix_seconds)
    }

    pub fn authorize_at_trusted(&self, unix_seconds: i64) -> AccessDecision {
        let mut observed = self.inner.trusted_floor.load(Ordering::SeqCst);
        loop {
            if unix_seconds.saturating_add(CLOCK_ROLLBACK_TOLERANCE) < observed {
                return AccessDecision::Denied(BlockReason::ClockRollback);
            }
            if unix_seconds <= observed {
                break;
            }
            match self.inner.trusted_floor.compare_exchange(
                observed,
                unix_seconds,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(current) => observed = current,
            }
        }
        self.authorize_at(unix_seconds.max(observed))
    }

    pub fn authorize_now(&self) -> AccessDecision {
        let Ok(duration) = SystemTime::now().duration_since(UNIX_EPOCH) else {
            return AccessDecision::Denied(BlockReason::ClockRollback);
        };
        let Ok(unix_seconds) = i64::try_from(duration.as_secs()) else {
            return AccessDecision::Denied(BlockReason::ClockRollback);
        };
        self.authorize_at_trusted(unix_seconds)
    }

    pub(crate) fn restore_trusted_time(&self, unix_seconds: i64) {
        self.inner
            .trusted_floor
            .fetch_max(unix_seconds, Ordering::SeqCst);
        self.mark_trusted_time_persisted(unix_seconds);
    }

    pub(crate) fn observed_trusted_time(&self) -> i64 {
        self.inner.trusted_floor.load(Ordering::SeqCst)
    }

    pub(crate) fn trusted_time_checkpoint_due(&self, interval_seconds: i64) -> Option<i64> {
        let observed = self.observed_trusted_time();
        let persisted = self.inner.persisted_trusted_floor.load(Ordering::SeqCst);
        (observed > 0 && observed.saturating_sub(persisted) >= interval_seconds).then_some(observed)
    }

    pub(crate) fn mark_trusted_time_persisted(&self, unix_seconds: i64) {
        self.inner
            .persisted_trusted_floor
            .fetch_max(unix_seconds, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::licensing::{AccessMode, LicenseLease, NeedsOnlineReason};

    fn lease() -> LicenseLease {
        LicenseLease::new("license-1", "machine-1", 10, 20).unwrap()
    }

    #[test]
    fn default_gate_denies_access() {
        assert_eq!(
            LicenseGate::default().authorize_at(15),
            AccessDecision::Denied(BlockReason::NotActivated)
        );
    }

    #[test]
    fn transition_is_visible_to_all_clones() {
        let gate = LicenseGate::default();
        let worker_gate = gate.clone();

        assert_eq!(
            worker_gate.transition_to(LicenseState::OnlineValid(lease())),
            1
        );
        assert_eq!(gate.snapshot().revision, 1);
        assert_eq!(
            gate.authorize_at(15),
            AccessDecision::Allowed {
                mode: AccessMode::Online,
                valid_until: 20,
            }
        );
    }

    #[test]
    fn offline_and_service_unavailable_use_bounded_lease() {
        for state in [
            LicenseState::OfflineLease(lease()),
            LicenseState::ServiceUnavailable(lease()),
        ] {
            let gate = LicenseGate::new(state);
            assert_eq!(
                gate.authorize_at(19),
                AccessDecision::Allowed {
                    mode: AccessMode::Offline,
                    valid_until: 20,
                }
            );
            assert_eq!(
                gate.authorize_at(20),
                AccessDecision::Denied(BlockReason::OfflineLeaseExpired)
            );
        }
    }

    #[test]
    fn every_blocking_state_denies_access() {
        let cases = [
            (LicenseState::Unlicensed, BlockReason::NotActivated),
            (LicenseState::Activating, BlockReason::ActivationInProgress),
            (
                LicenseState::NeedsOnline(NeedsOnlineReason::ClockRollback),
                BlockReason::ClockRollback,
            ),
            (LicenseState::Suspended, BlockReason::Suspended),
            (LicenseState::Expired, BlockReason::Expired),
            (
                LicenseState::HardwareMismatch,
                BlockReason::HardwareMismatch,
            ),
            (LicenseState::Tampered, BlockReason::Tampered),
        ];

        for (state, reason) in cases {
            assert_eq!(
                LicenseGate::new(state).authorize_at(15),
                AccessDecision::Denied(reason)
            );
        }
    }

    #[test]
    fn trusted_floor_rejects_clock_rollback_inside_session() {
        let gate = LicenseGate::new(LicenseState::OfflineLease(
            LicenseLease::new("license-1", "machine-1", 0, 1_000).unwrap(),
        ));

        assert!(gate.authorize_at_trusted(600).is_allowed());
        assert_eq!(
            gate.authorize_at_trusted(200),
            AccessDecision::Denied(BlockReason::ClockRollback)
        );
        assert!(gate.authorize_at_trusted(700).is_allowed());
    }

    #[test]
    fn trusted_floor_applies_clock_rollback_tolerance_to_lease_check() {
        let gate = LicenseGate::new(LicenseState::OfflineLease(
            LicenseLease::new("license-1", "machine-1", 1_000, 2_000).unwrap(),
        ));
        gate.restore_trusted_time(1_100);

        assert_eq!(
            gate.authorize_at_trusted(900),
            AccessDecision::Allowed {
                mode: AccessMode::Offline,
                valid_until: 2_000,
            }
        );
    }
}
