#[test]
fn cargo_toml_has_release_profile() {
    let cargo_toml = include_str!("../Cargo.toml");
    assert!(
        cargo_toml.contains("[profile.release]"),
        "Cargo.toml must have a [profile.release] section"
    );
    assert!(
        cargo_toml.contains("lto = true"),
        "Release profile must enable LTO"
    );
    assert!(
        cargo_toml.contains("strip = true"),
        "Release profile must strip symbols"
    );
    assert!(
        cargo_toml.contains("codegen-units = 1"),
        "Release profile must use single codegen unit"
    );
}

#[test]
fn release_workflow_exists() {
    let workflow = include_str!("../.github/workflows/build.yml");
    assert!(
        workflow.contains("x86_64-unknown-linux-gnu"),
        "Release workflow must target Linux x64"
    );
    assert!(
        workflow.contains("aarch64-apple-darwin"),
        "Release workflow must target macOS ARM64"
    );
    assert!(
        workflow.contains("x86_64-pc-windows-msvc"),
        "Release workflow must target Windows x64"
    );
}
