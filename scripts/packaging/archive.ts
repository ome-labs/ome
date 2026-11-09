#!/usr/bin/env node
// Assembles one platform's release archive, ome-<version>-<platform>.tar.gz, in <out>:
//
//   ome-<version>-<platform>/
//     bin/ome  bin/ome-keeper  bin/omed       (omed: a shell script that runs lib/omed with Node)
//     lib/omed/node_modules/                   (@ome-labs/daemon and its production dependencies, for this platform)
//     LICENSE  NOTICE  README.md
//
// The daemon is installed from the packed npm tarballs in <npm-dir> (scripts/packaging/npm.ts), so the archive holds
// exactly what npm users get. Run on the platform itself: some dependencies are native.
//
//   node scripts/packaging/archive.ts <platform> <bin-dir> <npm-dir> <out>
import { execFileSync } from "node:child_process";
import { chmodSync, copyFileSync, mkdirSync, readdirSync, rmSync, writeFileSync } from "node:fs";
import { join, resolve } from "node:path";

import { PACKAGES, args, platform, root, version } from "./targets.ts";

const [name, binArg, npmArg, outArg] = args("archive.ts", [
  "platform",
  "bin-dir",
  "npm-dir",
  "out",
]);
const target = platform(name);
const npmDir = resolve(npmArg);
const out = resolve(outArg);
const release = version();
const base = `ome-${release}-${target.name}`;
const dir = join(out, base);

rmSync(dir, { recursive: true, force: true });
mkdirSync(join(dir, "bin"), { recursive: true });
for (const program of ["ome", "ome-keeper"]) {
  copyFileSync(join(resolve(binArg), program), join(dir, "bin", program));
  chmodSync(join(dir, "bin", program), 0o755);
}
copyFileSync(join(root, "scripts/packaging/omed.sh"), join(dir, "bin/omed"));
chmodSync(join(dir, "bin/omed"), 0o755);

// Every packed library, so the daemon's @ome-labs dependencies resolve to this release's tarballs, not the registry.
const tarballs = readdirSync(npmDir)
  .filter((file) => file.endsWith(".tgz"))
  .filter((file) =>
    PACKAGES.some(
      (p) =>
        p.name !== "@ome-labs/ome" && file.startsWith(p.name.replace("@", "").replace("/", "-")),
    ),
  )
  .map((file) => join(npmDir, file));
const lib = join(dir, "lib/omed");
mkdirSync(lib, { recursive: true });
writeFileSync(join(lib, "package.json"), '{ "private": true }\n');
execFileSync(
  "npm",
  ["install", "--omit=dev", "--no-audit", "--no-fund", "--no-save", "--prefix", lib, ...tarballs],
  { stdio: "inherit" },
);
rmSync(join(lib, "package.json"));
rmSync(join(lib, "package-lock.json"), { force: true });

copyFileSync(join(root, "LICENSE"), join(dir, "LICENSE"));
copyFileSync(join(root, "NOTICE"), join(dir, "NOTICE"));
writeFileSync(
  join(dir, "README.md"),
  `# Ome ${release} for ${target.os} on ${target.cpu}

- \`bin/ome\`: the command line. Start with \`ome init\`, then \`ome doctor\`.
- \`bin/omed\`: the daemon \`ome\` starts for each profile. Needs Node 24 or later.
- \`bin/ome-keeper\`: the keeper the daemon runs, which holds your viewing share.

Install with \`curl -fsSL https://ome.network/install.sh | sh\` or \`brew install ome-labs/tap/ome\` rather than by hand.
Documentation: https://github.com/ome-labs/ome
`,
);

execFileSync("tar", ["-czf", `${base}.tar.gz`, base], { cwd: out, stdio: "inherit" });
rmSync(dir, { recursive: true, force: true });
process.stdout.write(`${join(out, `${base}.tar.gz`)}\n`);
