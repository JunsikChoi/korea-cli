use serial_test::serial;
use std::env;

#[test]
#[serial]
fn test_config_set_and_get() {
    let tmp = tempfile::tempdir().unwrap();
    env::set_var("XDG_CONFIG_HOME", tmp.path());

    let mut cfg = korea_cli::config::AppConfig::default();
    cfg.api_key = Some("test-key-123".to_string());
    cfg.save().unwrap();

    let loaded = korea_cli::config::AppConfig::load().unwrap();
    assert_eq!(loaded.api_key, Some("test-key-123".to_string()));
}

#[test]
#[serial]
fn test_env_var_takes_priority() {
    let cfg = korea_cli::config::AppConfig {
        api_key: Some("config-key".to_string()),
        ..Default::default()
    };

    env::set_var("DATA_GO_KR_API_KEY", "env-key");
    assert_eq!(cfg.resolve_api_key(), Some("env-key".to_string()));
    env::remove_var("DATA_GO_KR_API_KEY");

    assert_eq!(cfg.resolve_api_key(), Some("config-key".to_string()));
}

#[test]
#[serial]
fn test_no_key_returns_none() {
    env::remove_var("DATA_GO_KR_API_KEY");
    let cfg = korea_cli::config::AppConfig::default();
    assert_eq!(cfg.resolve_api_key(), None);
}
