fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get the workspace root (go up from cli/)
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")?;
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .ok_or("Failed to find workspace root")?;

    let proto_file = workspace_root.join("libs/trading_core/proto/trading_cli.proto");
    let proto_dir = workspace_root.join("libs/trading_core/proto");

    println!("cargo:rerun-if-changed={}", proto_file.display());
    println!("cargo:rerun-if-changed={}", proto_dir.display());

    // Generate gRPC client code from proto files
    tonic_build::configure()
        .build_client(true)
        .build_server(false)
        .compile(&[proto_file], &[proto_dir])?;

    Ok(())
}
