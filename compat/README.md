# Compatibility guards

What later releases must keep reading (`architecture/compatibility.md`).

- `vectors/v<N>.json`: the wire encodings of protocol version `N` as frozen. Every frozen message must still decode with
  the current code, verify, and re-encode to the same bytes (`crates/wire/tests/frozen.rs`); the current `v<N>`
  vectors must equal the frozen ones while `PROTOCOL_VERSION` is `N`.
- `fixtures/<release>/`: data a release wrote (a keeper store, a profile, a vault record), which the current code must
  open (`crates/keeper/tests/fixtures.rs`, `packages/daemon/src/profile/fixtures.test.ts`).
- `MODE`: `report` until the launch release, `enforce` from it. In report mode the guards print what changed and
  pass, since protocol `v0` is still open; `pnpm compat:freeze` re-freezes `v0` and adds this release's fixtures. In
  enforce mode a change fails CI: add a protocol version or a migration instead.
