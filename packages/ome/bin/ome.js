#!/usr/bin/env node
// `ome`: the native command line, told where this install's daemon and keeper are.
import { daemon, native } from "./native.js";
import { run } from "./run.js";

run(native("ome"), {
  OME_DAEMON: process.env.OME_DAEMON || daemon(),
  OME_KEEPER_BIN: process.env.OME_KEEPER_BIN || native("ome-keeper"),
});
