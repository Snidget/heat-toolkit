mod certificate;
mod gate;
mod hardware;
mod keygen_client;
mod manager;
mod model;
mod signature;
mod storage;

pub use certificate::{
    MachineCertificateError, MachineCertificateVerifier, MachineFileContext, VerifiedMachineFile,
    MACHINE_FILE_ALGORITHM,
};
pub use gate::{GateSnapshot, LicenseGate};
pub use hardware::{
    collect_hardware_identity, matches_stored_hardware, HardwareComponent, HardwareComponentKind,
    HardwareIdentity, HardwareIdentityError, HardwareIdentityStrength, HARDWARE_SCHEMA_VERSION,
};
pub use keygen_client::{
    CheckedOutMachine, KeygenClient, KeygenClientError, KeygenConfig, MachineActivation,
    ValidationCode, ValidationResult,
};
pub use manager::{LicenseManager, LicenseManagerSnapshot, LicenseOperation};
pub use model::{
    mask_license_key, AccessDecision, AccessMode, BlockReason, LeaseError, LicenseLease,
    LicenseState, NeedsOnlineReason,
};
pub use signature::{ResponseSignatureError, ResponseSignatureVerifier, SignedResponse};
pub use storage::{SecureLicenseRecord, SecureStore, StorageError, STORAGE_SCHEMA_VERSION};
