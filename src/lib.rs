pub mod core;
pub mod gui;

pub use core::{
    analysis::{
        add_peak_in_range, analyze_multiplet, auto_detect_integrations, auto_detect_reference_peak,
        compute_integral, estimate_noise_mad, pick_peaks, resolve_solvent_target_ppm,
        snap_and_add_peak, AutoSensitivity, IntegralResult, IntegrationItem, JCouplingCandidate,
        JCouplingResultItem, PeakItem, SolventInfo, KNOWN_SOLVENTS,
    },
    autophase::{acme_score, autophase_acme, nelder_mead_2d},
    baseline::{
        apply_baseline_correction, apply_baseline_method, baseline_airpls, baseline_als,
        baseline_polynomial, BaselineMethod,
    },
    error::{ResonaError, Result},
    io::{AcquisitionMetadata, BrukerReader, JeolJdfReader, NmrDataSource, RawFid},
    pipeline::{process_raw_fid, FtSettings, ProcessedSpectrum},
    project::{
        DisplaySettings, FullAutoReport, HistoryManager, MultiviewItem, Project, ProjectState,
        RectF,
    },
    signal::{
        apply_phase_and_extract_real, apply_phase_complex, apply_window, apply_zerofill,
        compute_ppm_scale, compute_window_curve, forward_fft, remove_fractional_delay,
        WindowFunction,
    },
};
pub use gui::ResonaApp;
