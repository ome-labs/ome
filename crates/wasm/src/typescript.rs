//! The TypeScript declarations of the boundary shapes. They are generated into `src/types.d.ts` (`pnpm --filter
//! @ome-labs/wasm generate`), which the module embeds in its own declarations; the shared shapes come from
//! `@ome-labs/api`.

use std::fmt::Write as _;

use ts_rs::{Config, TS};

use crate::types::{
    AddressContext, ApprovalInfo, Authwit, AuthwitWitness, BuiltProposal, Call, CallKind, Chain,
    CombineRequest, CombineResult, DealtCommittee, Draft, ExecutionRequest, ProposalInfo,
    RefusedVote, RejectionInfo, Rotation, TallyResult, TallyStatus, VaultInput, Warning,
};

/// What the declarations import from `@ome-labs/api`.
const SHARED: &str = "ErrorKind, Hex32, Hex64, HexBytes, Joiner, Manifest, MemberPartials, Point, Secret, VaultConfig";

fn declare<T: TS + 'static>(out: &mut String, cfg: &Config) {
    let docs = T::docs().unwrap_or_default();
    let _ = write!(out, "{docs}export {}\n\n", T::decl(cfg));
}

/// The generated file.
#[must_use]
pub fn render() -> String {
    let cfg = Config::new().with_large_int("number");
    let mut out = format!(
        "// Generated from crates/wasm by `pnpm --filter @ome-labs/wasm generate`. Do not edit.\n\n\
         import type {{ {SHARED} }} from \"@ome-labs/api\";\n\n"
    );
    declare::<VaultInput>(&mut out, &cfg);
    declare::<Chain>(&mut out, &cfg);
    declare::<AddressContext>(&mut out, &cfg);
    declare::<CombineRequest>(&mut out, &cfg);
    declare::<CombineResult>(&mut out, &cfg);
    declare::<CallKind>(&mut out, &cfg);
    declare::<Call>(&mut out, &cfg);
    declare::<Authwit>(&mut out, &cfg);
    declare::<ExecutionRequest>(&mut out, &cfg);
    declare::<Draft>(&mut out, &cfg);
    declare::<BuiltProposal>(&mut out, &cfg);
    declare::<ProposalInfo>(&mut out, &cfg);
    declare::<ApprovalInfo>(&mut out, &cfg);
    declare::<RejectionInfo>(&mut out, &cfg);
    declare::<TallyStatus>(&mut out, &cfg);
    declare::<RefusedVote>(&mut out, &cfg);
    declare::<AuthwitWitness>(&mut out, &cfg);
    declare::<TallyResult>(&mut out, &cfg);
    declare::<Rotation>(&mut out, &cfg);
    declare::<Warning>(&mut out, &cfg);
    declare::<DealtCommittee>(&mut out, &cfg);
    out.push_str(
        "/** What every function throws: an `Error` named `OmeWasmError` with the API error kind. */\n\
         export interface OmeWasmError extends Error {\n  name: \"OmeWasmError\";\n  kind: ErrorKind;\n}\n",
    );
    out
}
