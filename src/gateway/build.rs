fn main() -> Result<(), Box<dyn std::error::Error>> {
    let proto_root = "../../libs/trading_core/proto";
    let proto_file = format!("{}/trading_cli.proto", proto_root);

    tonic_build::configure()
        .build_client(true)
        .build_server(true)
        .compile(&[proto_file], &[proto_root])?;

    Ok(())
}
