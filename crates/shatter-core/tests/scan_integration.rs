use std::fs;
use std::time::Duration;

use shatter_core::{
    CancellationToken, Config, ConfigStore, DeleteRequest, DeleteStrategy, FileConfigStore,
    FsDeletionBackend, ScanRequest, ScanScope, ScanService, SizeMode,
};

fn create_service() -> ScanService {
    ScanService::from_config(Config::default())
}

#[test]
fn scan_finds_common_artifacts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("Cargo.toml");
    fs::create_dir_all(root.join("target/debug")).expect("target");
    fs::write(root.join("target/debug/app"), "hello").expect("artifact");

    let report = create_service()
        .scan(
            ScanRequest {
                roots: vec![root.to_path_buf()],
                scope: ScanScope::All,
                age_filter: None,
                protection_policy: shatter_core::ProtectionPolicy::RespectConfig,
                size_mode: SizeMode::Accurate,
            },
            None,
            CancellationToken::new(),
        )
        .expect("scan");

    assert_eq!(report.items.len(), 1);
    assert_eq!(report.items[0].rule_name, "target");
    assert!(report.items[0].bytes.unwrap_or_default() > 0);
}

#[test]
fn shatterignore_skips_subtree() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::write(root.join("package.json"), "{}").expect("package.json");
    fs::create_dir_all(root.join("build/cache")).expect("build");
    fs::write(root.join("build/.shatterignore"), "").expect(".shatterignore");

    let report = create_service()
        .scan(
            ScanRequest {
                roots: vec![root.to_path_buf()],
                scope: ScanScope::All,
                age_filter: None,
                protection_policy: shatter_core::ProtectionPolicy::RespectConfig,
                size_mode: SizeMode::Skip,
            },
            None,
            CancellationToken::new(),
        )
        .expect("scan");

    assert!(report.items.is_empty());
    assert!(!report.warnings.is_empty());
}

#[cfg(unix)]
#[test]
fn symlinks_are_not_followed() {
    use std::os::unix::fs::symlink;

    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("Cargo.toml");
    fs::create_dir_all(root.join("target")).expect("target");
    symlink(root.join("target"), root.join("linked-target")).expect("symlink");

    let report = create_service()
        .scan(
            ScanRequest {
                roots: vec![root.to_path_buf()],
                scope: ScanScope::All,
                age_filter: None,
                protection_policy: shatter_core::ProtectionPolicy::RespectConfig,
                size_mode: SizeMode::Skip,
            },
            None,
            CancellationToken::new(),
        )
        .expect("scan");

    assert_eq!(report.items.len(), 1);
}

#[test]
fn cancellation_produces_partial_report() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::write(root.join("package.json"), "{}").expect("package.json");
    for index in 0..100 {
        let path = root.join(format!("project-{index}/node_modules/cache"));
        fs::create_dir_all(&path).expect("node_modules");
        fs::write(path.join("item.bin"), vec![0_u8; 128]).expect("item.bin");
    }

    let token = CancellationToken::new();
    token.cancel();

    let report = create_service()
        .scan(
            ScanRequest {
                roots: vec![root.to_path_buf()],
                scope: ScanScope::All,
                age_filter: Some(Duration::from_secs(0)),
                protection_policy: shatter_core::ProtectionPolicy::RespectConfig,
                size_mode: SizeMode::Accurate,
            },
            None,
            token,
        )
        .expect("scan");

    assert!(report.cancelled);
}

#[test]
fn delete_selected_vs_all_works() {
    let temp = tempfile::tempdir().expect("tempdir");
    let root = temp.path();
    fs::write(
        root.join("Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )
    .expect("Cargo.toml");
    fs::create_dir_all(root.join("target/debug")).expect("target");
    fs::write(root.join("target/debug/app"), "hello").expect("artifact");
    fs::create_dir_all(root.join("node_modules/pkg")).expect("node_modules");
    fs::write(root.join("node_modules/pkg/index.js"), "export {}").expect("module");

    let report = create_service()
        .scan(
            ScanRequest {
                roots: vec![root.to_path_buf()],
                scope: ScanScope::All,
                age_filter: None,
                protection_policy: shatter_core::ProtectionPolicy::RespectConfig,
                size_mode: SizeMode::Skip,
            },
            None,
            CancellationToken::new(),
        )
        .expect("scan");

    let delete_service = shatter_core::DeleteService::new(FsDeletionBackend);
    let selected = report.items.first().cloned().expect("first item");
    let result = delete_service.delete(DeleteRequest {
        items: vec![selected],
        strategy: DeleteStrategy::Permanent,
    });

    assert_eq!(result.deleted.len(), 1);
    assert_eq!(result.failed.len(), 0);
    assert_eq!(fs::read_dir(root).expect("read_dir").count(), 2);
    assert!(FileConfigStore::default().load().is_ok());
}
