#!/usr/bin/env node
// `ome-keeper`: the native keeper, for running it by hand (a member's server); `omed` starts its own.
import { native } from "./native.js";
import { run } from "./run.js";

run(native("ome-keeper"));
