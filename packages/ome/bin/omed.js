#!/usr/bin/env node
// `omed`: the daemon from @ome-labs/daemon, in this process, told where this install's keeper is.
import { pathToFileURL } from "node:url";

import { daemon, native } from "./native.js";

process.env.OME_KEEPER_BIN ||= native("ome-keeper");
await import(pathToFileURL(daemon()).href);
