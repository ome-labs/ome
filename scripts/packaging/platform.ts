#!/usr/bin/env node
// Makes the npm package of one platform's native programs, @ome-labs/ome-<platform>, from a directory holding the
// built `ome` and `ome-keeper`, and packs it into <out>.
//
//   node scripts/packaging/platform.ts <platform> <bin-dir> <out>
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";

import { args, platform, root, version } from "./targets.ts";

const [name, binArg, outArg] = args("platform.ts", ["platform", "bin-dir", "out"]);
const target = platform(name);
const out = resolve(outArg);
const release = version();
const pkg = `@ome-labs/ome-${target.name}`;

const dir = mkdtempSync(join(tmpdir(), "ome-platform-"));
try {
  mkdirSync(join(dir, "bin"));
  for (const program of ["ome", "ome-keeper"]) {
    copyFileSync(join(resolve(binArg), program), join(dir, "bin", program));
    chmodSync(join(dir, "bin", program), 0o755);
  }
  copyFileSync(join(root, "LICENSE"), join(dir, "LICENSE"));
  const manifest = {
    name: pkg,
    version: release,
    description: `The ome command line and ome-keeper for ${target.os} on ${target.cpu}, installed by @ome-labs/ome`,
    keywords: ["ome", "aztec", "multisig"],
    homepage: "https://ome.network",
    bugs: "https://github.com/ome-labs/ome/issues",
    license: "Apache-2.0",
    author: "Ome Labs",
    repository: {
      type: "git",
      url: "git+https://github.com/ome-labs/ome.git",
      directory: "crates/cli",
    },
    os: [target.os],
    cpu: [target.cpu],
    files: ["bin", "LICENSE", "README.md"],
    publishConfig: { access: "public" },
  };
  writeFileSync(join(dir, "package.json"), `${JSON.stringify(manifest, null, 2)}\n`);
  writeFileSync(
    join(dir, "README.md"),
    `# ${pkg}

The native programs of [Ome](https://ome.network) for ${target.os} on ${target.cpu}: \`ome\`, the command line, and
\`ome-keeper\`, which holds a member's viewing share.

Install [\`@ome-labs/ome\`](https://www.npmjs.com/package/@ome-labs/ome) instead of this package: it picks the package
for your platform and runs these programs with the daemon they need.

\`\`\`sh
npm install --global @ome-labs/ome
\`\`\`
`,
  );
  mkdirSync(out, { recursive: true });
  execFileSync("npm", ["pack", "--pack-destination", out], { cwd: dir, stdio: "inherit" });
} finally {
  rmSync(dir, { recursive: true, force: true });
}
