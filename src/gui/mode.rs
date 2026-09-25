/// Resona GUI の操作モード定義

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    View,
    Zoom,
    Phase,
    Baseline,
    Reference,
    Peak,
    Integrate,
    Multiview,
    JCoupling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ZoomSubMode {
    #[default]
    Rect,
    X,
    Y,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PeakSubMode {
    #[default]
    Add,
    Delete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntegrateSubMode {
    #[default]
    Add,
    Edit,
    Split,
    Delete,
    Reference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MultiviewSubMode {
    #[default]
    AddRect,
    AddX,
    Edit,
    Delete,
}
