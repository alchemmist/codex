#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StartupMascotSkin {
    #[serde(rename = "ant-01")]
    Ant01,
    #[default]
    #[serde(rename = "ant-03")]
    Ant03,
    None,
}
