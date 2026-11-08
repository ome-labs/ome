//! Prints the `@ome-labs/api` declarations. Run through `pnpm --filter @ome-labs/api generate`, which formats them.

use std::io::Write as _;

fn main() -> std::io::Result<()> {
    std::io::stdout().write_all(ome_api::typescript::render().as_bytes())
}
