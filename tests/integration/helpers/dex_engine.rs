//! DeepBook / dex_engine integration test helpers.

use std::path::PathBuf;

/// Load a JSON fixture from `tests/integration/fixtures/dex_engine/`.
pub fn load_fixture(name: &str) -> String {
    let path = fixture_path(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()))
}

pub fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/integration/fixtures/dex_engine")
        .join(name)
}
