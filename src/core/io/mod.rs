pub mod bruker;
pub mod jdf;
pub mod traits;
pub mod varian;

pub use bruker::{compute_bruker_group_delay, BrukerReader, JcampValue};
pub use jdf::{compute_jeol_group_delay, JeolJdfReader};
pub use traits::{AcquisitionMetadata, NmrDataSource, RawFid};
pub use varian::{resolve_varian_paths, VarianReader};

