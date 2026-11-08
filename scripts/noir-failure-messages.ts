#!/usr/bin/env node
// Checks that every way a Noir package can refuse is tested: each `assert` / `assert_eq` message in `src/` must be
// expected verbatim by at least one `#[test(should_fail_with = "...")]` under `src/test/`.
//
// Template messages (`f"duplicate {label}"`) are expanded with the string literal each call site of the enclosing
// function passes last, so `"duplicate signer"` and `"duplicate committee"` are checked separately. An assertion no
// caller can reach is exempted by a comment directly above it containing `failure-check: invariant`, followed by the
// reason. An assertion without a message fails the check: a contract's failures must explain themselves.
//
// Usage: node scripts/noir-failure-messages.ts <package-dir>
// Example: node scripts/noir-failure-messages.ts contracts/ome_vault

import { readdirSync, readFileSync } from "node:fs";
import { join, relative } from "node:path";

const packageDir = process.argv[2];
if (!packageDir) {
  console.error("usage: noir-failure-messages.ts <package-dir>");
  process.exit(2);
}

/** A failure message and where the assertion raising it lives. */
interface Failure {
  message: string;
  location: string;
}

const sourceDir = join(packageDir, "src");
const testDir = join(sourceDir, "test");

/** Every `.nr` file under `dir`, recursively. */
function noirFiles(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return noirFiles(path);
    return entry.name.endsWith(".nr") ? [path] : [];
  });
}

/** The text between the parenthesis at `open` and its match, skipping string literals. */
function argumentsAt(text: string, open: number): string {
  let depth = 0;
  for (let i = open; i < text.length; i++) {
    const char = text[i];
    if (char === '"') {
      i++;
      while (i < text.length && text[i] !== '"') i += text[i] === "\\" ? 2 : 1;
    } else if (char === "(") {
      depth++;
    } else if (char === ")" && --depth === 0) {
      return text.slice(open + 1, i);
    }
  }
  return text.slice(open + 1);
}

/** The string literal an argument list ends with, if any. */
function trailingString(args: string): { template: boolean; value: string } | null {
  const match = args.trimEnd().match(/(f?)"((?:[^"\\]|\\.)*)"\s*,?$/);
  return match ? { template: match[1] === "f", value: match[2] ?? "" } : null;
}

const lineOf = (text: string, index: number): number => text.slice(0, index).split("\n").length;

/** Whether the comment block right above `line` (1-based) marks the assertion as unreachable. */
function isMarkedInvariant(lines: string[], line: number): boolean {
  for (let i = line - 2; i >= 0 && lines[i]?.trim().startsWith("//"); i--) {
    if (lines[i]?.includes("failure-check: invariant")) return true;
  }
  return false;
}

const sources = noirFiles(sourceDir)
  .filter((path) => !path.startsWith(testDir))
  .map((path) => ({ path, text: readFileSync(path, "utf8") }));

/** The last string literal passed at every call of `fn`, across the package sources. */
function literalsPassedTo(fn: string): string[] {
  const values = new Set<string>();
  for (const { text } of sources) {
    for (const call of text.matchAll(new RegExp(`\\b${fn}\\s*\\(`, "g"))) {
      const isDefinition = /\bfn\s*$/.test(text.slice(0, call.index));
      const literal = trailingString(argumentsAt(text, call.index + call[0].length - 1));
      if (!isDefinition && literal) values.add(literal.value);
    }
  }
  return [...values];
}

const failures: Failure[] = [];
const exempt: Failure[] = [];
const unexplained: string[] = [];

for (const { path, text } of sources) {
  const lines = text.split("\n");

  for (const assertion of text.matchAll(/\b(assert|assert_eq)\s*\(/g)) {
    const line = lineOf(text, assertion.index);
    if (lines[line - 1]?.trim().startsWith("//")) continue;

    const location = `${relative(packageDir, path)}:${line}`;
    const message = trailingString(argumentsAt(text, assertion.index + assertion[0].length - 1));

    if (!message) {
      unexplained.push(location);
    } else if (isMarkedInvariant(lines, line)) {
      exempt.push({ message: message.value, location });
    } else if (message.template) {
      const fn = [...text.slice(0, assertion.index).matchAll(/\bfn\s+(\w+)/g)].at(-1)?.[1];
      if (!fn) throw new Error(`template assertion outside a function at ${location}`);

      for (const value of literalsPassedTo(fn)) {
        failures.push({ message: message.value.replace(/\{\w+\}/g, value), location });
      }
    } else {
      failures.push({ message: message.value, location });
    }
  }
}

const expected = new Map<string, number>();
for (const path of noirFiles(testDir)) {
  for (const match of readFileSync(path, "utf8").matchAll(
    /should_fail_with\s*=\s*"((?:[^"\\]|\\.)*)"/g,
  )) {
    const message = match[1] ?? "";
    expected.set(message, (expected.get(message) ?? 0) + 1);
  }
}

const rows = failures.map(({ message, location }) => ({
  message,
  tests: expected.get(message) ?? 0,
  location,
}));
console.table(rows);

for (const { message, location } of exempt) {
  console.log(`exempt (invariant): "${message}" at ${location}`);
}

const untested = rows.filter(({ tests }) => tests === 0);
for (const { message, location } of untested) {
  console.error(`untested failure: "${message}" at ${location}`);
}
for (const location of unexplained) {
  console.error(`assertion without a message at ${location}`);
}

if (untested.length > 0 || unexplained.length > 0) process.exit(1);
console.log(`All ${rows.length} failure messages are expected by at least one test.`);
