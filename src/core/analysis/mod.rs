pub mod integral;
pub mod jcoupling;
pub mod peak;

pub use integral::{auto_detect_integrations, compute_integral, IntegralResult, IntegrationItem};
pub use jcoupling::{analyze_multiplet, JCouplingCandidate, JCouplingResultItem};
pub use peak::{estimate_noise_mad, pick_peaks, snap_and_add_peak, PeakItem};
