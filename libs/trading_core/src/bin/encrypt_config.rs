//! Simple binary to encrypt config files (called by encrypt_config.sh)

use std::env;
use tracing::{error, info};

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() != 4 {
        error!("Usage: encrypt_config <input> <output> <password>");
        std::process::exit(1);
    }

    let input = &args[1];
    let output = &args[2];
    let password = &args[3];

    match trading_core::config::encrypt_config_file(input, output, password) {
        Ok(_) => info!("✓ Config encrypted: {}", output),
        Err(e) => {
            error!("✗ Encryption failed: {}", e);
            std::process::exit(1);
        }
    }
}
