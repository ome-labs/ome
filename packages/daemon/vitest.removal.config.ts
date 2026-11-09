import defineNodeVitestConfig from "klarity/vitest/node";

// The member removal drill (`pnpm test:removal`) against `aztec start --local-network`: two vaults, removals, and what
// the removed member's old state can still do. Kept out of the default end-to-end run for its length.
export default defineNodeVitestConfig({
  test: {
    include: ["test/removal/**/*.test.ts"],
    fileParallelism: false,
    sequence: { concurrent: false },
    testTimeout: 1_200_000,
    hookTimeout: 900_000,
  },
});
