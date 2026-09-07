#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Quota {
    pub primary: Option<QuotaWindow>,
    pub secondary: Option<QuotaWindow>,
    pub credits: Option<Credits>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuotaWindow {
    pub used_basis_points: u16,
    pub window_seconds: Option<u64>,
    pub resets_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Credits {
    pub available: bool,
    pub unlimited: bool,
    pub balance: Option<String>,
}
