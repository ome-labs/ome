import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["tests/**/*.test.ts"],
    // Argon2id in WebAssembly takes a few seconds per unseal.
    testTimeout: 60_000,
  },
});
