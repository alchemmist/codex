use antex_runtime::Config;
use antex_runtime::PermissionProfile;
use pretty_assertions::assert_eq;

#[test]
fn config_loads_supported_fields_and_reports_unknown_names_without_values() {
    let home = tempfile::tempdir().unwrap();
    std::fs::write(home.path().join("config.toml"),"model = 'fake'\nmodel_reasoning_effort = 'high'\npermissions = 'read-only'\nshell_timeout_seconds = 600\nunknown = 'secret-value'\n").unwrap();
    let loaded = Config::load(home.path()).unwrap();
    assert_eq!(
        loaded.config,
        Config {
            model: Some("fake".into()),
            model_reasoning_effort: Some("high".into()),
            permissions: PermissionProfile::ReadOnly,
            shell_timeout_seconds: 600
        }
    );
    assert_eq!(
        loaded.warnings,
        vec!["unsupported configuration key: unknown"]
    );
}

#[test]
fn invalid_config_errors_never_echo_source_values() {
    let home = tempfile::tempdir().unwrap();
    for text in [
        "model = secret-value",
        "model = ['secret-value']",
        "shell_timeout_seconds = 0",
    ] {
        std::fs::write(home.path().join("config.toml"), text).unwrap();
        let error = Config::load(home.path()).err().unwrap();
        assert!(!error.to_string().contains("secret-value"));
    }
}
