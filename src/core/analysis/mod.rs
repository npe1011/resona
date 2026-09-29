pub mod integral;
pub mod jcoupling;
pub mod peak;
pub mod reference;
pub mod sensitivity;

pub use integral::{auto_detect_integrations, compute_integral, IntegralResult, IntegrationItem};
pub use jcoupling::{analyze_multiplet, JCouplingCandidate, JCouplingResultItem};
pub use peak::{add_peak_in_range, estimate_noise_mad, pick_peaks, snap_and_add_peak, PeakItem};
pub use reference::{
    auto_detect_reference_peak, resolve_solvent_target_ppm, SolventInfo, KNOWN_SOLVENTS,
};
pub use sensitivity::AutoSensitivity;
