import { BackendType, Barretenberg } from "@aztec-foundation/bb.js";

/**
 * Initialises the Barretenberg singleton with an explicit backend. Must run before the PXE is created: Aztec's
 * Poseidon helpers otherwise initialise it with defaults first and the PXE's own backend option is ignored.
 * `NativeSharedMemory` is not offered: it crashes on large messages.
 */
export async function initBarretenberg(
  backend: BackendType.NativeUnixSocket | BackendType.Wasm = BackendType.NativeUnixSocket,
): Promise<void> {
  await Barretenberg.initSingleton({ backend });
}
