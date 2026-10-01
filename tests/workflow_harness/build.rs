fn main() -> Result<(), Box<dyn std::error::Error>> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .ok_or("workflow_harness must live at tests/workflow_harness")?;
    let proto_file = workspace_root.join("libs/trading_core/proto/trading_cli.proto");
    let proto_dir = workspace_root.join("libs/trading_core/proto");
    println!("cargo:rerun-if-changed={}", proto_file.display());
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .compile(&[&proto_file], &[&proto_dir])?;
    Ok(())
}
