fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the workspace root (go up from src/runner/)
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Failed to find workspace root")?;

    let proto_file = workspace_root.join("libs/trading_core/proto/trading_cli.proto");
    let proto_dir = workspace_root.join("libs/trading_core/proto");

    println!("cargo:rerun-if-changed={}", proto_file.display());
    println!("cargo:rerun-if-changed={}", proto_dir.display());

    // Generate gRPC code from proto files
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&[proto_file], &[proto_dir])?;

    Ok(())
}
