import defineNodeVitestConfig from "klarity/vitest/node";

// Builds the ome-relay and ome-keeper images and runs a relay and three keepers on separate Docker networks (Docker must be up).
export default defineNodeVitestConfig({
  test: {
    include: ["test/**/*.test.ts"],
    fileParallelism: false,
    sequence: { concurrent: false },
    testTimeout: 600_000,
    hookTimeout: 1_800_000,
  },
});
