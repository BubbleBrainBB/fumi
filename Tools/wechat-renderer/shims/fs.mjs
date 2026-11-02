// node:fs for the WeChat renderer inside Fumi: reads go to the app
// (globalThis.__fumiHost); the skill's own files come from the bundle.
const SKILL = '/__skill/';
const host = () => globalThis.__fumiHost;

/** Bytes of a file, read lazily: render.mjs only hashes them or takes base64. */
export class FileBytes {
  constructor(path) { this.path = path; }
  toString(encoding) {
    return encoding === 'base64' ? host().readBase64(this.path) : host().readText(this.path);
  }
}

const skillAsset = p => host().skillAsset(p.slice(SKILL.length));

export function readFileSync(p, encoding) {
  if (p.startsWith(SKILL)) {
    const text = skillAsset(p);
    if (text == null) throw new Error(`ENOENT: ${p}`);
    return text;
  }
  if (!host().isFile(p)) throw new Error(`ENOENT: ${p}`);
  return encoding ? host().readText(p) : new FileBytes(p);
}

export function existsSync(p) {
  return p.startsWith(SKILL) ? skillAsset(p) != null : host().fileExists(p);
}

export function statSync(p) {
  const isFile = p.startsWith(SKILL) ? skillAsset(p) != null : host().isFile(p);
  return { isFile: () => isFile, isDirectory: () => !isFile && existsSync(p) };
}

export const realpathSync = p => p;
export const lstatSync = () => ({ isSymbolicLink: () => false });
export function mkdirSync() { throw new Error('Fumi: the renderer does not write files.'); }
export function writeFileSync() { throw new Error('Fumi: the renderer does not write files.'); }

export default { readFileSync, existsSync, statSync, realpathSync, lstatSync, mkdirSync, writeFileSync };
