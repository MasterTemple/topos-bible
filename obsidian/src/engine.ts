import init, { Topos } from "topos-bible";
import wasm from "topos-bible-wasm";

/**
 * Loads the embedded WebAssembly module and creates the engine.
 * esbuild points `topos-bible` at the package's core module (no automatic loading) and embeds
 * the `.wasm` file, so this works offline and on mobile.
 */
export async function loadTopos(): Promise<Topos> {
  await init(wasm);
  return Topos.new();
}

/** The engine's WebAssembly, to start background workers with */
export const engineWasm: Uint8Array = wasm;
