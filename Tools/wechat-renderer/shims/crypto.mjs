// node:crypto — SHA-256 only, computed by the app.
import { FileBytes } from './fs.mjs';

export function createHash(algorithm) {
  if (algorithm !== 'sha256') throw new Error(`Fumi: unsupported hash ${algorithm}`);
  let value = '';
  return {
    update(input) { value = input; return this; },
    digest() {
      const host = globalThis.__fumiHost;
      return value instanceof FileBytes ? host.sha256File(value.path) : host.sha256Text(String(value));
    },
  };
}

export default { createHash };
