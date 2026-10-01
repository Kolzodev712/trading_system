//! Retired. This binary did not install a `test_open_order` guard and must not
//! be treated as a no-order safety mode. Isolated Paper workflows live in
//! `just test-paper` / `workflow_gate`.

fn main() {
    eprintln!(
        "test_runner is retired. It never installed a no-order guard and must not be used against any venue.\n\
         Use `just test-paper` for isolated Paper process workflows.\n\
         Bybit Demo mutating workflows require `just test-demo-mutating` and are never implied by this binary."
    );
    std::process::exit(2);
}
