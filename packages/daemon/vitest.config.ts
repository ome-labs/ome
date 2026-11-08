import defineNodeVitestConfig from "klarity/vitest/node";
import { configDefaults } from "vitest/config";

export default defineNodeVitestConfig({
  // End-to-end tests need a local network; they run in their own lane (`vitest.e2e.config.ts`).
  test: { exclude: [...configDefaults.exclude, "test/e2e/**"] },
});
