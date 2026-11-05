#!/usr/bin/env node
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { MarkdownIt, parseYaml } from '../vendor/parsers.mjs';
import { prepareVideos, videoChecklist } from './videos.mjs';
import { prepareFlows } from './flows.mjs';

const skillDir = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const themeFiles = { 'black-red': 'theme.json', 'blueprint': 'themes/blueprint.json', 'songti-supplement': 'themes/songti-supplement.json', 'motion-journal': 'themes/motion-journal.json' };
const canonicalTheme = name => name === 'paper-notes' ? 'blueprint' : name;
export function loadTheme(name = 'black-red') {
  name = canonicalTheme(name);
  if (!Object.hasOwn(themeFiles, name)) throw new Error(`未知主题：${name}；可选 black-red、blueprint、songti-supplement、motion-journal。`);
  return JSON.parse(fs.readFileSync(path.join(skillDir, 'assets', themeFiles[name]), 'utf8'));
}
const hash = value => createHash('sha256').update(value).digest('hex');
const escape = value => String(value).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const wrap = (tag, style, content, attrs = '') => `<${tag} style="${escape(style)}"${attrs}>${content}</${tag}>`;
const marker = '<p style="display:none;"><mp-style-type data-value="3"></mp-style-type></p>';

export function renderArticle(raw, { inputPath = path.resolve('article.md'), imageMap = {}, theme: themeName = 'black-red', toc = false, scrollLongQuotes = false } = {}) {
  themeName = canonicalTheme(themeName);
  const theme = loadTheme(themeName);
  const s = theme.styles;
  if (scrollLongQuotes && !s.scrollQuote) throw new Error('当前主题不支持长引用滚动区块。');
  let scrollQuoteCount = 0;
  const warnings = new Set();
  const images = [];
  const md = new MarkdownIt({ html: false, breaks: true, linkify: false, typographer: false });
  if (!imageMap || typeof imageMap !== 'object' || Array.isArray(imageMap)) throw new Error('图片映射必须是 JSON 对象：原始路径 -> HTTPS 图床地址。');
  const mappedImages = new Map();
  for (const [source, target] of Object.entries(imageMap)) {
    let url;
    try { url = new URL(target); } catch { throw new Error(`图床映射地址无效：${source}`); }
    if (typeof target !== 'string' || url.protocol !== 'https:' || url.username || url.password) throw new Error(`图床映射必须使用不带账号密码的 HTTPS 图片直链：${source}`);
    mappedImages.set(md.normalizeLink(source), md.normalizeLink(target));
  }
  let markdown = raw.replace(/^\uFEFF/, '').replace(/\r\n?/g, '\n');
  let metaTitle = '';
  let lineOffset = 0;
  if (markdown.startsWith('---\n')) {
    const match = markdown.match(/^---\n([\s\S]*?)\n(?:---|\.\.\.)(?:\n|$)/);
    if (!match) throw new Error('YAML frontmatter 没有结束分隔线。');
    const meta = parseYaml(match[1]);
    if (typeof meta?.title === 'string' || typeof meta?.title === 'number') metaTitle = String(meta.title).trim();
    markdown = markdown.slice(match[0].length);
    lineOffset = (match[0].match(/\n/g) || []).length;
  }

  const tags = [
    ['<mark data-tone="warning">', '</mark>', 'warning'],
    ['<span data-tone="red">', '</span>', 'red'],
    ['<mark>', '</mark>', 'highlight'],
    ['<u>', '</u>', 'underline'],
    ['==', '==', 'highlight'],
  ];
  md.inline.ruler.before('html_inline', 'fixed_emphasis', (state, silent) => {
    for (const [open, close, style] of tags) {
      if (!state.src.startsWith(open, state.pos)) continue;
      const end = state.src.indexOf(close, state.pos + open.length);
      if (end < 0 || end === state.pos + open.length) continue;
      if (silent) return true;
      const token = state.push('fixed_emphasis', 'span', 0);
      token.meta = { style };
      token.children = [];
      md.inline.parse(state.src.slice(state.pos + open.length, end), md, state.env, token.children);
      state.pos = end + close.length;
      return true;
    }
    const br = state.src.slice(state.pos).match(/^<br\s*\/?>/i);
    if (br) {
      if (!silent) { state.push('hardbreak', 'br', 0); state.pos += br[0].length; }
      return true;
    }
    return false;
  });
  // Register before text so == is seen even in the middle of Chinese prose.
  md.inline.ruler.before('text', 'fixed_equals', (state, silent) => {
    if (!state.src.startsWith('==', state.pos)) return false;
    const end = state.src.indexOf('==', state.pos + 2);
    if (end <= state.pos + 2) return false;
    if (!silent) {
      const token = state.push('fixed_emphasis', 'span', 0);
      token.meta = { style: 'highlight' };
      token.children = [];
      md.inline.parse(state.src.slice(state.pos + 2, end), md, state.env, token.children);
      state.pos = end + 2;
    }
    return true;
  });
  // markdown-it's text scanner does not treat '=' as a boundary.
  const defaultText = md.inline.ruler.getRules('').find(rule => rule.name === 'text');
  md.inline.ruler.at('text', (state, silent) => {
    const next = state.src.indexOf('==', state.pos);
    const end = state.posMax;
    if (next > state.pos) state.posMax = next;
    const result = defaultText(state, silent);
    state.posMax = end;
    return result;
  });
  md.renderer.rules.fixed_emphasis = (tokens, i, options, env, self) =>
    wrap('span', s[tokens[i].meta.style], self.renderInline(tokens[i].children, options, env));

  const env = { sections: 0 };
  let tokens = md.parse(markdown, env);
  let title = metaTitle || path.basename(inputPath, path.extname(inputPath));
  let titleExtracted = false;
  if (tokens[0]?.type === 'heading_open' && tokens[0].tag === 'h1') {
    title = md.renderer.renderInlineAsText(tokens[1].children, md.options, env).trim();
    titleExtracted = true;
    if (metaTitle && metaTitle !== title) warnings.add('frontmatter 标题与开头 H1 不同；使用原文 H1 作为 title.txt，元数据标题未进入正文。');
    tokens = tokens.slice(3);
  }
  const animations = prepareFlows(md, tokens, { lineOffset });
  if (animations.length) warnings.add(`含 ${animations.length} 个 SVG 动画；本地预览可播放，公众号接收、手机预览和发布效果尚未验证。`);
  const videos = prepareVideos(md, tokens, { inputPath, lineOffset, styles: s });
  if (videos.length) warnings.add(`有 ${videos.length} 处视频待在公众号后台插入；复制的正文包含占位卡，完成插入后请删除对应卡片。`);
  for (const video of videos) {
    if (video.kind === 'local' && !video.exists) warnings.add(`视频 ${video.number} 的本地文件未找到：${video.source}`);
  }

  function resolveImage(src) {
    const original = src;
    src = mappedImages.get(src) || src;
    if (src.startsWith('//')) src = `https:${src}`;
    if (/^https?:\/\//i.test(src)) {
      images.push({ source: original, url: src, kind: 'remote', mapped: src !== original, loadStatus: 'pending-browser-check' });
      if (/^http:/i.test(src)) warnings.add('存在 HTTP 图片，建议使用 HTTPS 并检查公众号接收结果。');
      return src;
    }
    if (/^data:image\/(png|jpe?g|gif|webp);base64,[a-z0-9+/=\s]+$/i.test(src)) {
      images.push({ source: '(embedded data image)', kind: 'embedded' });
      warnings.add('含 Base64 图片，将随正文复制；请在公众号粘贴后检查图片是否接收成功。');
      return src;
    }
    if (/^(?:[a-z][a-z0-9+.-]*:|\/\/|#)/i.test(src)) throw new Error(`不支持的图片地址：${src}`);
    let decoded;
    try { decoded = decodeURIComponent(src); } catch { throw new Error(`图片路径编码无效：${src}`); }
    const resolved = path.resolve(path.dirname(inputPath), decoded);
    const ext = path.extname(resolved).toLowerCase();
    const mime = { '.png': 'image/png', '.jpg': 'image/jpeg', '.jpeg': 'image/jpeg', '.gif': 'image/gif', '.webp': 'image/webp' }[ext];
    if (!mime) throw new Error(`本地图片仅支持 PNG/JPEG/GIF/WebP：${src}`);
    if (!fs.existsSync(resolved) || !fs.statSync(resolved).isFile()) throw new Error(`图片文件不存在：${resolved}`);
    const content = fs.readFileSync(resolved);
    images.push({ source: src, kind: 'local', sha256: hash(content) });
    warnings.add('本地图片已嵌入正文，可一同复制；请在公众号粘贴后检查图片是否接收成功。');
    return `data:${mime};base64,${content.toString('base64')}`;
  }

  function inspectInline(children) {
    for (const token of children || []) {
      if (token.type === 'image') token.attrSet('src', resolveImage(token.attrGet('src') || ''));
      if (token.type === 'link_open') {
        const href = token.attrGet('href') || '';
        if (!/^https?:\/\//i.test(href)) warnings.add('含非 HTTP(S) 链接：地址保持原样，公众号中可能不可用。');
      }
      if (token.type === 'text') {
        if (/!?\[\[/.test(token.content)) warnings.add('Obsidian Wiki 链接或嵌入未转换，作为可见原文保留。');
        if (/\[\^[^\]]+\]/.test(token.content)) warnings.add('脚注语法未转换，作为可见原文保留。');
        if (/\$[^$\n]+\$|\$\$|\\\(|\\\[/.test(token.content)) warnings.add('数学公式未排版，作为可见原文保留。');
        if (/<\/?[a-z][^>]*>/i.test(token.content)) warnings.add('存在未支持的 HTML，已转义为可见文本，未执行或应用其样式。');
      }
      inspectInline(token.children);
    }
  }
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (token.type === 'blockquote_open') {
      const first = tokens[i + 2];
      const callout = first?.type === 'inline' && first.content.match(/^\[!([A-Za-z][\w-]*)\][+-]?(?:[ \t]+([^\n]*))?(?:\n|$)/);
      if (callout) {
        const kind = callout[1].toUpperCase();
        const titleTokens = [];
        md.inline.parse(callout[2] || '', md, env, titleTokens);
        inspectInline(titleTokens);
        token.meta = { callout: kind, titleTokens };
        const close = tokens.findIndex((t, n) => n > i && t.type === 'blockquote_close' && t.level === token.level);
        tokens[close].meta = token.meta;
        first.content = first.content.slice(callout[0].length).trim();
        first.children = [];
        md.inline.parse(first.content, md, env, first.children);
        if (!first.content) {
          tokens[i + 1].hidden = true;
          tokens[i + 3].hidden = true;
        }
      }
    }
    if (scrollLongQuotes && token.type === 'blockquote_open' && token.level === 0 && !token.meta?.callout) {
      const close = tokens.findIndex((t, n) => n > i && t.type === 'blockquote_close' && t.level === token.level);
      const inner = tokens.slice(i + 1, close);
      const text = inner.filter(t => t.type === 'inline').map(t => md.renderer.renderInlineAsText(t.children || [], md.options, env)).join('');
      // Only plain, top-level quotations; nested quotes and callouts retain their semantics.
      if (!inner.some(t => t.type === 'blockquote_open') && [...text].length > 300) {
        token.meta = { ...token.meta, scrollQuote: true };
        tokens[close].meta = token.meta;
        scrollQuoteCount++;
        warnings.add('长引用已改为 240px 高的滚动区块，正文完整保留；公众号是否保留滚动以及字数限制需实际粘贴验证。');
      }
    }
    if (token.type === 'inline') {
      inspectInline(token.children);
      const visible = token.children.filter(t => !['softbreak', 'hardbreak'].includes(t.type) && !(t.type === 'text' && !t.content.trim()));
      if (visible.length && visible.every(t => t.type === 'image') && tokens[i - 1]?.type === 'paragraph_open') {
        tokens[i - 1].meta = { imageOnly: true };
        tokens[i + 1].meta = { imageOnly: true };
        for (const image of visible) image.meta = { block: true };
      }
    }
  }

  const tocEntries = [];
  if (toc) {
    // Use parsed blocks so examples inside code and inline comments remain literal.
    const slots = [];
    for (let i = 0; i < tokens.length; i++) {
      if (tokens[i].type === 'paragraph_open' && tokens[i].level === 0 &&
          tokens[i + 1]?.type === 'inline' && tokens[i + 1].content.trim() === '<!-- toc -->' &&
          tokens[i + 2]?.type === 'paragraph_close') slots.push(i);
    }
    if (slots.length > 1) throw new Error('只能放置一个独立的 <!-- toc --> 目录标记。');
    const plainText = children => (children || []).map(t => t.children ? plainText(t.children) :
      ['text', 'code_inline'].includes(t.type) ? t.content : ['softbreak', 'hardbreak'].includes(t.type) ? ' ' : '').join('');
    for (let i = 0; i < tokens.length; i++) {
      if (tokens[i].type !== 'heading_open' || tokens[i].tag !== 'h2') continue;
      const number = String(tocEntries.length + 1).padStart(2, '0');
      const id = `article-section-${number}`;
      tokens[i].attrSet('id', id);
      tocEntries.push({ id, number, title: plainText(tokens[i + 1]?.children) });
    }
    const slot = slots[0] ?? tokens.findIndex(t => t.type === 'heading_open' && t.tag === 'h2');
    if (tocEntries.length) {
      const token = new tokens[slot].constructor('article_toc', 'section', 0);
      token.block = true;
      tokens.splice(slot, slots.length ? 3 : 0, token);
    } else {
      if (slots.length) tokens.splice(slot, 3);
      warnings.add('没有二级标题，未生成目录。');
    }
  }
  md.renderer.rules.article_toc = () => wrap('section', s.tocWrap,
    wrap('p', s.tocTitle, '本文目录') + wrap('section', s.tocScroll,
      tocEntries.map(entry => wrap('a', s.tocLink,
        wrap('span', s.tocNumber, entry.number) + wrap('span', s.tocText, escape(entry.title)),
        ` href="#${entry.id}"`)).join(''), ' data-toc-scroll="true" tabindex="0" role="navigation" aria-label="文章目录"'),
    ' data-article-toc="true"') + '\n';

  const renderToken = (tokens, i, options, env, self) => self.renderToken(tokens, i, options);
  for (const [type, style] of Object.entries({ thead_open: 'tableSection', tbody_open: 'tableSection', tr_open: 'tableRow' })) {
    if (!s[style]) continue;
    md.renderer.rules[type] = (tokens, i, options, env, self) => {
      tokens[i].attrSet('style', s[style]);
      return renderToken(tokens, i, options, env, self);
    };
  }
  for (const [type, style] of Object.entries({ strong_open: 'strong', em_open: 'em', s_open: 'strike', link_open: 'link', bullet_list_open: 'list', ordered_list_open: 'list', list_item_open: 'listItem', th_open: 'th', td_open: 'td' })) {
    md.renderer.rules[type] = (tokens, i, options, env, self) => {
      // Table alignment is source structure; append it to the fixed cell style.
      const alignment = tokens[i].attrGet('style') || '';
      tokens[i].attrSet('style', s[style] + alignment);
      return renderToken(tokens, i, options, env, self);
    };
  }
  md.renderer.rules.heading_open = (tokens, i) => {
    const tag = tokens[i].tag;
    if (tag === 'h2') env.sections++;
    const marker = tag === 'h2' && theme.headingNumbered
      ? wrap('span', s.headingNumber, String(env.sections).padStart(2, '0'), ' data-theme-ornament="heading" aria-hidden="true"')
      : tag === 'h3' && s.h3Marker ? wrap('span', s.h3Marker, escape(theme.h3MarkerText || '/'), ' data-theme-ornament="subheading" aria-hidden="true"') : '';
    const anchor = tokens[i].attrGet('id');
    return `<${tag} style="${escape(s[tag] + (anchor ? 'scroll-margin-top:24px;' : ''))}"${anchor ? ` id="${escape(anchor)}"` : ''}>${marker}`;
  };
  md.renderer.rules.heading_close = (tokens, i) => `</${tokens[i].tag}>\n`;
  md.renderer.rules.paragraph_open = (tokens, i) => {
    if (tokens[i].hidden || tokens[i].meta?.imageOnly) return '';
    const quote = tokens.slice(0, i).reduce((count, t) => count + (t.type === 'blockquote_open' ? 1 : t.type === 'blockquote_close' ? -1 : 0), 0);
    return `<p style="${escape(quote ? s.quoteParagraph : s.paragraph)}">`;
  };
  md.renderer.rules.paragraph_close = (tokens, i) => tokens[i].hidden || tokens[i].meta?.imageOnly ? '\n' : '</p>\n';
  md.renderer.rules.blockquote_open = (tokens, i) => {
    if (tokens[i].meta?.scrollQuote) return `<section style="${escape(s.scrollQuote)}" data-scroll-quote="true">` + wrap('p', s.scrollQuoteLabel, '上下滑动查看完整内容') + `<section style="${escape(s.scrollQuoteBody)}" tabindex="0" role="region" aria-label="完整引用内容，可上下滚动">`;
    const kind = tokens[i].meta?.callout;
    if (!kind) return `<blockquote style="${escape(s.quote)}">` + (s.quoteMark ? wrap('span', s.quoteMark, '“', ' data-theme-ornament="quote" aria-hidden="true"') : '');
    const titleTokens = tokens[i].meta.titleTokens;
    const title = titleTokens.length ? wrap('p', s.calloutTitle, md.renderer.renderInline(titleTokens, md.options, env)) : '';
    return `<section style="${escape(kind === 'END' ? s.end : s.callout)}">${title}`;
  };
  md.renderer.rules.blockquote_close = (tokens, i) => tokens[i].meta?.scrollQuote ? '</section></section>\n' : tokens[i].meta?.callout ? '</section>\n' : '</blockquote>\n';
  md.renderer.rules.image = (tokens, i, options, env, self) => {
    const token = tokens[i];
    const alt = self.renderInlineAsText(token.children || [], options, env);
    const imageTitle = token.attrGet('title');
    const img = `<img src="${escape(token.attrGet('src'))}" alt="${escape(alt)}"${imageTitle ? ` title="${escape(imageTitle)}"` : ''} referrerpolicy="no-referrer" style="${escape(s.image)}" />`;
    return token.meta?.block ? wrap('figure', s.figure, img + (alt ? wrap('figcaption', s.caption, escape(alt)) : '')) : img;
  };
  md.renderer.rules.code_inline = (tokens, i) => wrap('code', s.inlineCode, escape(tokens[i].content));
  function code(tokens, i) {
    const info = tokens[i].info || '';
    const language = info.trim().split(/\s+/)[0] || '';
    const dots = wrap('span', s.codeDots,
      ['codeDotRed', 'codeDotYellow', 'codeDotGreen'].map(color => wrap('span', s.codeDot + s[color], '●')).join(''));
    const label = language ? wrap('span', s.codeLang, escape(language.toUpperCase())) : '';
    const header = theme.codePresentation === 'index-card'
      ? wrap('section', s.codeHeader, wrap('span', s.codeLang, escape(language.toUpperCase() || 'CODE')))
      : theme.codePresentation === 'plain'
      ? (label ? wrap('section', s.codeHeader, label) : '')
      : wrap('section', s.codeHeader, dots + label);
    return wrap('section', s.codeWrap, header + wrap('pre', s.pre, wrap('code', s.code, escape(tokens[i].content)))) + '\n';
  }
  md.renderer.rules.fence = code;
  md.renderer.rules.code_block = code;
  md.renderer.rules.hr = () => `<hr style="${escape(s.hr)}" />\n`;
  md.renderer.rules.table_open = () => `<section style="${escape(s.tableWrap)}"><table style="${escape(s.table)}">`;
  md.renderer.rules.table_close = () => '</table></section>\n';
  const body = md.renderer.render(tokens, md.options, env);
  const html = wrap('section', s.root, body + marker, ` data-template="${theme.id}"`) + '\n';
  const report = {
    template: theme.id, theme: themeName, version: theme.version,
    sourceSha256: hash(raw), templateSha256: hash(JSON.stringify(theme)),
    title, leadingTitleExtracted: titleExtracted, sections: env.sections,
    images, videos, animations, scrollQuotes: { enabled: Boolean(scrollLongQuotes), count: scrollQuoteCount }, toc: { enabled: Boolean(toc), entries: tocEntries }, warnings: [...warnings], requiresReview: warnings.size > 0,
    imageCopyEligible: images.every(image => ['remote', 'local', 'embedded'].includes(image.kind)),
    imageLoadCheck: images.length ? 'pending-browser-check' : 'not-needed',
    wechatPasteVerified: false,
  };
  return { html, title, report };
}

export function buildPreview({ html, title, report }) {
  const theme = loadTheme(report.theme);
  const template = fs.readFileSync(path.join(skillDir, 'assets/preview.html'), 'utf8');
  const payload = JSON.stringify({ html, title, warnings: report.warnings, imageCopyEligible: report.imageCopyEligible, videos: report.videos, videoChecklist: videoChecklist(report.videos) }).replace(/</g, '\\u003c');
  // Preview-only color mapping; exported and copied HTML retains the inline light palette.
  const darkColors = theme.preview.darkColors || {};
  const darkCss = Object.values(theme.styles).map(style => {
    const declarations = style.split(';').filter(value => /#[0-9a-f]{6}/i.test(value))
      .map(value => value.replace(/#[0-9a-f]{6}/gi, color => darkColors[color.toLowerCase()] || color) + ' !important').join(';');
    return declarations ? `.paper.dark [style^=${JSON.stringify(style)}]{${declarations}}` : '';
  }).join('\n');
  const shell = template.replace('/*DARK_PREVIEW_STYLES*/', () => darkCss)
    .replace('<!--DARK_PREVIEW_HIDDEN-->', Object.keys(darkColors).length ? '' : 'hidden')
    .replaceAll('<!--THEME_NAME-->', escape(theme.name))
    .replaceAll('/*ACCENT*/#8f1d22', theme.preview.accent)
    .replaceAll('/*HOVER*/#f5eeee', theme.preview.hover)
    .replaceAll('/*PAPER*/white', theme.preview.paper)
    .replace('<!--PREVIEW_TITLE_STYLE-->', () => escape(theme.preview.titleStyle || ''));
  return shell.replace('<!--TITLE-->', () => escape(title)).replace('<!--DATA-->', () => payload).replace('<!--ARTICLE-->', () => html);
}

function main() {
  const args = process.argv.slice(2);
  if (args.includes('--help')) {
    console.log('node render.mjs --input /path/article.md --out-dir /path/output [--theme black-red|blueprint|songti-supplement|motion-journal] [--image-map /path/images.json] [--toc] [--scroll-long-quotes] [--overwrite]\n固定主题：black-red 黑红刊刻风（默认）、blueprint 蓝图档案、songti-supplement 宋体副刊、motion-journal 动态实验刊；旧参数 paper-notes 映射到 blueprint。--toc 开启固定高度滚动目录，可用独立 <!-- toc --> 指定位置，默认在首个 H2 前；复制时保留滚动样式。图片映射为 JSON 对象：原始路径 -> HTTPS 图床地址。未映射本地图片将嵌入正文并允许复制，公众号接收效果需实测。流程动画使用 wechat-flow 围栏（YAML title/steps/returnLabel，四节点固定布局）。视频使用 wechat-video 围栏（YAML title/source），或独立视频链接/嵌入。输出 article.html、preview.html、title.txt、report.json、videos.md、videos.json；视频保留为待插入占位卡，不上传或嵌入播放器。');
    return;
  }
  const options = {};
  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--overwrite') { options.overwrite = true; continue; }
    if (args[i] === '--scroll-long-quotes') { options.scrollLongQuotes = true; continue; }
    if (args[i] === '--toc') { options.toc = true; continue; }
    if (!['--input', '--out-dir', '--image-map', '--theme'].includes(args[i]) || !args[i + 1] || args[i + 1].startsWith('--')) throw new Error(`未知参数或缺少值：${args[i]}`);
    options[args[i].slice(2)] = args[++i];
  }
  if (!options.input || !options['out-dir']) throw new Error('必须提供 --input 与 --out-dir；运行 --help 查看用法。');
  const input = fs.realpathSync(path.resolve(options.input));
  const out = path.resolve(options['out-dir']);
  const raw = fs.readFileSync(input, 'utf8');
  const imageMap = options['image-map'] ? JSON.parse(fs.readFileSync(path.resolve(options['image-map']), 'utf8')) : {};
  const result = renderArticle(raw, { inputPath: input, imageMap, theme: options.theme, toc: options.toc, scrollLongQuotes: options.scrollLongQuotes });
  const files = {
    'article.html': result.html, 'preview.html': buildPreview(result),
    'title.txt': result.title + '\n', 'report.json': JSON.stringify(result.report, null, 2) + '\n',
    'videos.md': videoChecklist(result.report.videos), 'videos.json': JSON.stringify(result.report.videos, null, 2) + '\n',
  };
  for (const [name, content] of Object.entries(files)) {
    const dest = path.join(out, name);
    if (dest === input || (fs.existsSync(dest) && fs.realpathSync(dest) === input)) throw new Error('输出不能覆盖源文。');
    if (fs.existsSync(dest) && fs.lstatSync(dest).isSymbolicLink()) throw new Error(`输出不能是符号链接：${dest}`);
    if (!options.overwrite && fs.existsSync(dest) && fs.readFileSync(dest, 'utf8') !== content) throw new Error(`输出已有不同内容，使用新目录或 --overwrite：${dest}`);
  }
  fs.mkdirSync(out, { recursive: true });
  for (const [name, content] of Object.entries(files)) fs.writeFileSync(path.join(out, name), content);
  console.log(JSON.stringify({ output: out, files: Object.keys(files), ...result.report }, null, 2));
}

if (process.argv[1] && fs.existsSync(process.argv[1]) && fs.realpathSync(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { main(); } catch (error) { console.error(`排版失败：${error.message}`); process.exitCode = 1; }
}
