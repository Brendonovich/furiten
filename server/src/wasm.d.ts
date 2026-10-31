// Alchemy's Worker bundler uploads `.wasm` imports as CompiledWasm modules, which workerd hands over pre-compiled.
declare module "*.wasm" {
  const module: WebAssembly.Module;
  export default module;
}
