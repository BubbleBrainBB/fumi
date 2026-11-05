// node:path (POSIX) — the parts the WeChat renderer uses.
function normalizeParts(parts, absolute) {
  const out = [];
  for (const part of parts) {
    if (!part || part === '.') continue;
    if (part === '..') { if (out.length && out[out.length - 1] !== '..') out.pop(); else if (!absolute) out.push('..'); }
    else out.push(part);
  }
  return out;
}

export function normalize(p) {
  const absolute = p.startsWith('/');
  const joined = normalizeParts(p.split('/'), absolute).join('/');
  return (absolute ? '/' : '') + joined || (absolute ? '/' : '.');
}

export function join(...parts) { return normalize(parts.filter(Boolean).join('/')); }

export function resolve(...parts) {
  let resolved = '';
  for (let i = parts.length - 1; i >= 0 && !resolved.startsWith('/'); i--) {
    if (parts[i]) resolved = parts[i] + (resolved ? '/' + resolved : '');
  }
  if (!resolved.startsWith('/')) resolved = '/' + resolved;
  return normalize(resolved);
}

export function dirname(p) {
  const trimmed = p.length > 1 ? p.replace(/\/+$/, '') : p;
  const index = trimmed.lastIndexOf('/');
  if (index < 0) return '.';
  return index === 0 ? '/' : trimmed.slice(0, index);
}

export function basename(p, ext) {
  let base = p.replace(/\/+$/, '').split('/').pop() || '';
  if (ext && base.endsWith(ext) && base !== ext) base = base.slice(0, -ext.length);
  return base;
}

export function extname(p) {
  const base = basename(p);
  const index = base.lastIndexOf('.');
  return index <= 0 ? '' : base.slice(index);
}

export const sep = '/';
export default { normalize, join, resolve, dirname, basename, extname, sep };
