/**
 * `@ome-labs/daemon`: `omed`, an Ome member's background service (`architecture/daemon.md`). Apps talk to it through
 * the daemon API of `@ome-labs/api`, over its owner-only socket.
 */
export { DaemonCallError, DaemonClient } from "./client.ts";
export { DAEMON_VERSION, Daemon, type DaemonOptions } from "./daemon.ts";
export { DaemonError, toRpcError } from "./errors.ts";
export { type ProfilePaths, omeHome, profilePaths } from "./profile/paths.ts";
