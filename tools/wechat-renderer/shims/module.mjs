// node:module — the vendored yaml asks for 'buffer' and 'process'.
export function createRequire() {
  return id => {
    if (id === 'process') return globalThis.process;
    if (id === 'buffer') return {};
    throw new Error(`Fumi: ${id} is not available to the renderer`);
  };
}
export default { createRequire };
