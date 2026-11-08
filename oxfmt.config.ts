import config from "klarity/oxfmt";
import { defineConfig } from "oxfmt";

export default defineConfig({
  ...config,
  ignorePatterns: [
    ...(config.ignorePatterns ?? []),
    // Golden vectors are written by @ome-labs/vectors; reformatting them would make every regeneration a diff.
    "crates/*/tests/vectors/**",
  ],
});
