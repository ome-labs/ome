#!/usr/bin/env node
// omed: runs one member's daemon for one profile until `daemon.stop` or a signal.
//
//   omed [--profile <name>] [--home <dir>]
import { parseArgs } from "node:util";

import { Daemon } from "./daemon.ts";

const { values } = parseArgs({
  options: {
    profile: { type: "string", default: process.env.OME_PROFILE || "default" },
    home: { type: "string" },
  },
});

const daemon = new Daemon({
  profile: values.profile as string,
  ...(values.home ? { home: values.home } : {}),
});
await daemon.start();
process.stdout.write(`${JSON.stringify({ ready: daemon.info() })}\n`);

let stopping = false;
const stop = () => {
  if (stopping) return;
  stopping = true;
  void daemon.stop().finally(() => process.exit(0));
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
