fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_dir = "proto";
    let proto_file = "proto/trading_cli.proto";

    println!("cargo:rerun-if-changed={proto_file}");

    // Generate both client and server stubs once; consumers pick what they need.
    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile(&[proto_file], &[proto_dir])?;

    Ok(())
}
