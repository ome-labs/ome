import defineNodeVitestConfig from "klarity/vitest/node";

// Runs against `aztec start --local-network` (AZTEC_NODE_URL, default http://localhost:8080): three or four daemons,
// each with its own keeper process and PXE, in one test process. Tests in a file share their vault and run in order.
export default defineNodeVitestConfig({
  test: {
    include: ["test/e2e/**/*.test.ts"],
    fileParallelism: false,
    sequence: { concurrent: false },
    testTimeout: 900_000,
    hookTimeout: 900_000,
  },
});
