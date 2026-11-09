// Three members use Ome through `ome` only, against `aztec start --local-network`: each sets up a profile, they found
// a 2-of-3 vault, pay from it with two approvals, and remove a member (a reshare, then `rotate_membership`). Every
// step is a command a person would type, with prompts off (`--no-input`, `--yes`) and the passphrase from
// OME_PASSPHRASE_FILE.
//
//   node --test scripts/test-cli.ts      (via `pnpm test:cli`, which builds what it needs first)

import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { mkdtemp, rm, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";
import { after, before, test } from "node:test";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..");
const OME = join(root, "crates", "target", "debug", "ome");
const NODE = process.env.AZTEC_NODE_URL ?? "http://localhost:8080";

let home = "";
const env = () => ({
  ...process.env,
  OME_HOME: home,
  OME_PASSPHRASE_FILE: join(home, "passphrase"),
});

interface Run {
  code: number | null;
  stdout: string;
  stderr: string;
}

/** Runs `ome` as `profile`; `watch` is called with stderr as it grows, for commands that print something to pass on. */
function ome(profile: string, args: string[], watch?: (stderr: string) => void): Promise<Run> {
  return new Promise((resolve, reject) => {
    const child = spawn(OME, ["--profile", profile, ...args], { env: env() });
    let stdout = "";
    let stderr = "";
    child.stdout.on("data", (chunk: Buffer) => {
      stdout += chunk.toString();
    });
    child.stderr.on("data", (chunk: Buffer) => {
      stderr += chunk.toString();
      watch?.(stderr);
    });
    child.on("error", reject);
    child.on("close", (code) => resolve({ code, stdout, stderr }));
  });
}

/** Runs `ome` and expects it to succeed. */
async function ok(profile: string, args: string[]): Promise<Run> {
  const run = await ome(profile, args);
  assert.equal(run.code, 0, `ome ${args.join(" ")} failed:\n${run.stderr}`);
  return run;
}

async function json<T>(profile: string, args: string[]): Promise<T> {
  return JSON.parse((await ok(profile, ["--json", ...args])).stdout) as T;
}

/** Starts a long command and resolves the `<prefix>…` value it prints on stderr, with the command's own end. */
function passOn(profile: string, args: string[], prefix: string) {
  const pending: { resolve?: (value: string) => void } = {};
  const value = new Promise<string>((resolve) => {
    pending.resolve = resolve;
  });
  const done = ome(profile, args, (stderr) => {
    const match = new RegExp(`${prefix}[A-Za-z0-9_:-]+`, "u").exec(stderr);
    if (match) pending.resolve?.(match[0]);
  });
  return { value, done };
}

const PEOPLE = [
  ["alice", "Alice"],
  ["bob", "Bob"],
  ["carol", "Carol"],
] as const;

before(async () => {
  home = await mkdtemp(join(tmpdir(), "ome-cli-"));
  await writeFile(join(home, "passphrase"), "correct horse battery staple\n", { mode: 0o600 });
});

after(async () => {
  await Promise.all(PEOPLE.map(([profile]) => ome(profile, ["daemon", "stop"])));
  await rm(home, { recursive: true, force: true });
});

test("each member sets up a profile", async () => {
  await Promise.all(
    PEOPLE.map(([profile, name]) =>
      ok(profile, ["init", "--name", name, "--node-url", NODE, "--no-input"]),
    ),
  );
  const status = await json<{ profile: { name: string }; vaults: unknown[] }>("alice", ["status"]);
  assert.equal(status.profile.name, "Alice");
  assert.deepEqual(status.vaults, []);
});

let vault = "";

test("they found a 2-of-3 vault: Alice creates it, Bob and Carol join with her invite", async () => {
  const create = passOn(
    "alice",
    ["vault", "create", "--name", "Treasury", "--members", "3", "-k", "2", "-t", "2", "--json"],
    "ome-invite-v0:",
  );
  const invite = await create.value;
  const joins = ["bob", "carol"].map((profile) => ok(profile, ["vault", "join", invite, "--yes"]));
  const [created] = await Promise.all([create.done, ...joins]);
  assert.equal(created?.code, 0, created?.stderr);
  const summary = JSON.parse(created?.stdout ?? "") as { address: string; threshold: number };
  assert.equal(summary.threshold, 2);
  vault = summary.address;
  for (const [profile] of PEOPLE) {
    // oxlint-disable-next-line no-await-in-loop
    const [listed] = await json<{ address: string }[]>(profile, ["vault", "list"]);
    assert.equal(listed?.address, vault);
  }
});

test("they pay from it: proposed, reviewed and approved by two, executed", async () => {
  const token = await json<{ address: string; symbol: string }>("alice", ["token", "deploy-test"]);
  await ok("bob", ["token", "add", token.address]);
  const bob = await json<{ profile: { account: string } }>("bob", ["status"]);
  await ok("alice", ["contact", "add", bob.profile.account, "bob"]);

  const dry = await json<{ ok: boolean }>("alice", [
    "propose",
    "transfer",
    "25",
    token.symbol,
    "bob",
    "--dry-run",
  ]);
  assert.equal(dry.ok, true);
  const proposal = await json<{ id: string }>("alice", [
    "propose",
    "transfer",
    "25",
    token.symbol,
    "contact:bob",
    "--memo",
    "rent",
    "--yes",
  ]);
  const short = proposal.id.slice(2, 10);

  const review = await json<{ checks: { rebuilt: boolean; failure: string | null } }>("bob", [
    "proposal",
    "review",
    short,
  ]);
  assert.equal(review.checks.rebuilt, true);
  assert.equal(review.checks.failure, null);

  // Executing before the threshold is refused with the reason and exit code 4 (not ready).
  await ok("alice", ["approve", short, "--yes"]);
  const early = await ome("alice", ["execute", short]);
  assert.equal(early.code, 4);
  assert.match(early.stderr, /1 of 2 approvals/u);

  const ready = await json<{ status: string }>("bob", ["approve", short, "--yes"]);
  assert.equal(ready.status, "ready");
  const executed = await json<{ status: string; execution: { txHash: string } }>("alice", [
    "execute",
    short,
  ]);
  assert.equal(executed.status, "executed");
  const [balance] = await json<{ balance: string }[]>("alice", ["balance", token.symbol]);
  assert.equal(BigInt(balance?.balance ?? "0"), 975n * 10n ** 18n);
});

test("they remove Carol: a reshare and rotate_membership, approved by the two who stay", async () => {
  const change = passOn(
    "alice",
    [
      "member",
      "change",
      "--remove",
      "Carol",
      "-k",
      "2",
      "-t",
      "2",
      "--confirm",
      "Treasury",
      "--yes",
      "--json",
    ],
    "ome-rotation-v0:",
  );
  const plan = await change.value;
  const joined = await ome("bob", ["member", "join", plan, "--yes"]);
  assert.equal(joined.code, 0, joined.stderr);
  const changed = await change.done;
  assert.equal(changed.code, 0, changed.stderr);
  assert.equal((JSON.parse(changed.stdout) as { error: string | null }).error, null);

  const members = await json<{ name: string }[]>("alice", ["vault", "members"]);
  assert.deepEqual(
    members.map((m) => m.name),
    ["Alice", "Bob"],
  );
  const counterparties = await json<unknown[]>("alice", ["counterparties"]);
  assert.ok(Array.isArray(counterparties));
});
