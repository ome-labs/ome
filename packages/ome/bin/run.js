// Runs a native program in this process's terminal and exits with its status. A Ctrl-C reaches it directly (it shares
// the terminal's process group), so this process only has to wait.
import { spawnSync } from "node:child_process";

/**
 * @param {string} path
 * @param {Record<string, string>} [env]
 * @returns {never}
 */
export function run(path, env = {}) {
  const result = spawnSync(path, process.argv.slice(2), {
    stdio: "inherit",
    env: { ...process.env, ...env },
  });
  if (result.error) {
    process.stderr.write(`ome: could not run ${path}: ${result.error.message}\n`);
    process.exit(1);
  }
  process.exit(result.status ?? 1);
}
