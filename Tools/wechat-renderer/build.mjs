// Bundles Fumi's WeChat renderer (src/, forked from the wechat-black-red
// skill at black-red theme 1.4.0 and maintained here on its own) into one
// script Fumi runs in JavaScriptCore, and copies its themes and licenses
// beside it.
//
//     cd Tools/wechat-renderer && npm install && node build.mjs
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import esbuild from 'esbuild';

const here = path.dirname(fileURLToPath(import.meta.url));
const repo = path.resolve(here, '../..');
const src = path.join(here, 'src');
const out = path.join(repo, 'Fumi/Resources/WeChat');

const shims = { fs: 'fs.mjs', path: 'path.mjs', crypto: 'crypto.mjs', url: 'url.mjs', module: 'module.mjs' };
const nodeShims = {
  name: 'fumi-node-shims',
  setup(build) {
    build.onResolve({ filter: /^node:/ }, args => {
      const file = shims[args.path.slice(5)];
      if (!file) throw new Error(`No shim for ${args.path}`);
      return { path: path.join(here, 'shims', file) };
    });
  },
};

// The skill's markdown-it gets markdown-it-cjk-friendly, so bold and italic
// beside Chinese punctuation (这是**“重点”**内容) render as the editor shows
// them. The skill's files stay as they are: its scripts import this wrapper
// in place of vendor/parsers.mjs.
const parsers = path.join(src, 'vendor/parsers.mjs');
const cjkFriendly = {
  name: 'fumi-cjk-friendly',
  setup(build) {
    build.onResolve({ filter: /vendor\/parsers\.mjs$/ }, args =>
      args.namespace === 'fumi' ? undefined : { path: 'parsers', namespace: 'fumi' });
    build.onLoad({ filter: /.*/, namespace: 'fumi' }, () => ({
      contents: `import { MarkdownIt as Base, parseYaml } from ${JSON.stringify(parsers)};
import cjkFriendly from 'markdown-it-cjk-friendly';
export function MarkdownIt(options) { return new Base(options).use(cjkFriendly); }
export { parseYaml };`,
      resolveDir: here,
    }));
  },
};

await esbuild.build({
  stdin: {
    contents: `import { renderArticle } from ${JSON.stringify(path.join(src, 'scripts/render.mjs'))};
globalThis.FumiWeChat = {
  render(markdown, options) {
    const { html, title, report } = renderArticle(markdown, options);
    return JSON.stringify({ html, title, warnings: report.warnings, images: report.images.length, videos: report.videos.length });
  },
};`,
    resolveDir: here,
    loader: 'js',
  },
  absWorkingDir: repo,
  bundle: true,
  format: 'iife',
  platform: 'neutral',
  target: 'safari16',
  banner: { js: fs.readFileSync(path.join(here, 'globals.js'), 'utf8') },
  plugins: [nodeShims, cjkFriendly],
  outfile: path.join(out, 'wechat-render.js'),
  logLevel: 'warning',
  // render.mjs reads import.meta only to find itself (shimmed in url.mjs).
  logOverride: { 'empty-import-meta': 'silent' },
});

for (const file of ['assets/theme.json', 'assets/themes/blueprint.json', 'assets/themes/songti-supplement.json',
                    'assets/themes/motion-journal.json', 'vendor/THIRD_PARTY_LICENSES.txt']) {
  const target = path.join(out, file);
  fs.mkdirSync(path.dirname(target), { recursive: true });
  fs.copyFileSync(path.join(src, file), target);
}
// Licenses of what Fumi adds to the skill's bundle.
const added = ['markdown-it-cjk-friendly', 'get-east-asian-width'].map(name => {
  const folder = path.join(here, 'node_modules', name);
  const license = fs.readdirSync(folder).find(file => /^licen[cs]e/i.test(file));
  return `${name}\n${'='.repeat(name.length)}\n\n${fs.readFileSync(path.join(folder, license), 'utf8').trim()}\n`;
});
fs.writeFileSync(path.join(out, 'vendor/FUMI_ADDED_LICENSES.txt'), added.join('\n'));
const version = JSON.parse(fs.readFileSync(path.join(src, 'assets/theme.json'), 'utf8')).version;
fs.writeFileSync(path.join(out, 'SOURCE.txt'),
  `Bundled from Tools/wechat-renderer/src (forked from the wechat-black-red skill; black-red theme ${version}).\nRebuild with: cd Tools/wechat-renderer && node build.mjs\n`);
console.log(`Wrote ${path.relative(repo, out)}`);
