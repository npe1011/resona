pub mod analysis;
pub mod autophase;
pub mod baseline;
pub mod error;
pub mod io;
pub mod pipeline;
pub mod project;
pub mod signal;

pub use analysis::{
    analyze_multiplet, auto_detect_integrations, compute_integral, estimate_noise_mad,
    pick_peaks, snap_and_add_peak, IntegralResult, IntegrationItem, JCouplingCandidate,
    JCouplingResultItem, PeakItem,
};
pub use autophase::{acme_score, autophase_acme, nelder_mead_2d};
pub use baseline::{apply_baseline_correction, baseline_als};
pub use error::{ResonaError, Result};
pub use io::{AcquisitionMetadata, JeolJdfReader, NmrDataSource, RawFid};
pub use pipeline::{process_raw_fid, FtSettings, ProcessedSpectrum};
pub use project::{DisplaySettings, HistoryManager, MultiviewItem, Project, ProjectState, RectF};
pub use signal::{
    apply_phase_and_extract_real, apply_phase_complex, apply_window, apply_zerofill,
    compute_ppm_scale, compute_window_curve, forward_fft, remove_fractional_delay, WindowFunction,
};
