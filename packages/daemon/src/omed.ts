#!/usr/bin/env node
// omed: runs one member's daemon for one profile until `daemon.stop` or a signal.
//
//   omed [--profile <name>] [--home <dir>]
//
// OME_APPS_PORT serves the app page on 127.0.0.1 at that port, a development transport for app connections.
import { parseArgs } from "node:util";

import { Daemon } from "./daemon.ts";

const { values } = parseArgs({
  options: {
    profile: { type: "string", default: process.env.OME_PROFILE || "default" },
    home: { type: "string" },
  },
});

const appsPort = process.env.OME_APPS_PORT ? Number(process.env.OME_APPS_PORT) : undefined;
if (appsPort !== undefined && !(Number.isInteger(appsPort) && appsPort >= 0 && appsPort <= 65_535))
  throw new Error("OME_APPS_PORT is a port number");
const daemon = new Daemon({
  profile: values.profile as string,
  ...(values.home ? { home: values.home } : {}),
  ...(appsPort === undefined ? {} : { appsPort }),
});
await daemon.start();
process.stdout.write(
  `${JSON.stringify({ ready: daemon.info(), appPage: daemon.appPage ?? null })}\n`,
);

let stopping = false;
const stop = () => {
  if (stopping) return;
  stopping = true;
  void daemon.stop().finally(() => process.exit(0));
};
process.on("SIGTERM", stop);
process.on("SIGINT", stop);
