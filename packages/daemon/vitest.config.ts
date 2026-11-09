import defineNodeVitestConfig from "klarity/vitest/node";
import { configDefaults } from "vitest/config";

export default defineNodeVitestConfig({
  // End-to-end tests and the removal drill need a local network; they run in their own lanes
  // (`vitest.e2e.config.ts`, `vitest.removal.config.ts`).
  test: { exclude: [...configDefaults.exclude, "test/e2e/**", "test/removal/**"] },
});
