/** The topos-bible WebAssembly module, embedded by esbuild's binary loader */
declare module "topos-bible-wasm" {
  const bytes: Uint8Array;
  export default bytes;
}
