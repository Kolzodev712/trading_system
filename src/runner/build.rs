fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_file = "../../proto/trading_cli.proto";
    let proto_dir = "../../proto";

    println!("cargo:rerun-if-changed={}", proto_file);
    println!("cargo:rerun-if-changed={}", proto_dir);

    // Generate gRPC code from proto files
    tonic_build::configure()
        .build_server(true)
        .build_client(true)
        .compile(&[proto_file], &[proto_dir])?;

    Ok(())
}
