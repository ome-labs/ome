//! Prints the boundary declarations. Run through `pnpm --filter @ome-labs/wasm generate`, which formats them.

use std::io::Write as _;

fn main() -> std::io::Result<()> {
    std::io::stdout().write_all(ome_wasm::typescript::render().as_bytes())
}
