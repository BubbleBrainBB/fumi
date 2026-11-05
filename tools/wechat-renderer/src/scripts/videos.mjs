import fs from 'node:fs';
import path from 'node:path';
import { parseYaml } from '../vendor/parsers.mjs';

const escape = value => String(value).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const markdownEscape = value => String(value).replace(/([\\`*_{}\[\]<>#!|])/g, '\\$1').replace(/[\r\n]+/g, ' ');

function isVideoSource(source) {
  if (/\.(mp4|mov|m4v|webm|avi|mkv)(?:[?#]|$)/i.test(source)) return true;
  try {
    const url = new URL(source);
    return /^(?:www\.)?(?:youtu\.be|b23\.tv)$/.test(url.hostname)
      || /^(?:www\.)?youtube\.com$/.test(url.hostname) && /^\/(watch|shorts\/)/.test(url.pathname)
      || /^(?:www\.)?bilibili\.com$/.test(url.hostname) && /^\/video\//.test(url.pathname)
      || url.hostname === 'v.qq.com' && /^\/(?:x\/(?:page|cover)\/|iframe\/player)/.test(url.pathname);
  } catch { return false; }
}

function sourceInfo(source, inputPath) {
  if (/^https?:\/\//i.test(source)) {
    const url = new URL(source);
    if (url.username || url.password) throw new Error('视频来源链接不能包含账号密码。');
    return { kind: 'remote', href: source };
  }
  const windowsPath = /^[a-z]:[\\/]/i.test(source);
  if (/^[a-z][a-z0-9+.-]*:/i.test(source) && !windowsPath) throw new Error(`视频来源不支持此协议：${source}`);
  if (isVideoSource(source) && !/[\uFF1A]/.test(source)) {
    let decoded;
    try { decoded = decodeURIComponent(source); } catch { decoded = source; }
    const resolvedPath = path.resolve(path.dirname(inputPath), decoded);
    return { kind: 'local', resolvedPath, exists: fs.existsSync(resolvedPath) && fs.statSync(resolvedPath).isFile() };
  }
  return { kind: 'reference' };
}

// Convert only explicit video fences and complete, standalone video links/embeds.
// Ordinary code blocks and links inside sentences retain their original meaning.
export function prepareVideos(md, tokens, { inputPath, lineOffset = 0, styles }) {
  const videos = [];
  let section = '正文开头';
  function register({ source, title }, token) {
    if (typeof source !== 'string' || !source.trim()) throw new Error('wechat-video 标记必须提供非空 source（链接、本地路径或视频号定位信息）。');
    if (title != null && typeof title !== 'string') throw new Error('wechat-video 的 title 必须是文字。');
    source = source.trim();
    const number = String(videos.length + 1).padStart(2, '0');
    const video = {
      id: `video-${number}`, number, title: title?.trim() || `视频 ${number}`,
      source, ...sourceInfo(source, inputPath), section,
      sourceLine: token.map ? token.map[0] + lineOffset + 1 : null,
      status: 'pending-wechat-insertion',
    };
    videos.push(video);
    token.type = 'video_placeholder';
    token.meta = { video };
  }
  for (let i = 0; i < tokens.length; i++) {
    const token = tokens[i];
    if (token.type === 'heading_open' && /^h[12]$/.test(token.tag)) {
      section = md.renderer.renderInlineAsText(tokens[i + 1]?.children || [], md.options, {});
    }
    if (token.type === 'fence' && token.info.trim() === 'wechat-video') {
      const data = parseYaml(token.content);
      if (!data || typeof data !== 'object' || Array.isArray(data)) throw new Error('wechat-video 内容必须是 YAML 对象（title / source）。');
      if (Object.keys(data).some(key => !['title', 'source'].includes(key))) throw new Error('wechat-video 仅支持 title 和 source，其他信息请保留在标记外的正文中。');
      register(data, token);
      continue;
    }
    if (token.type !== 'paragraph_open' || tokens[i + 1]?.type !== 'inline' || tokens[i + 2]?.type !== 'paragraph_close') continue;
    const children = tokens[i + 1].children || [];
    let candidate;
    if (children.length === 1 && children[0].type === 'image') {
      const image = children[0];
      const source = image.attrGet('src') || '';
      if (isVideoSource(source)) candidate = { source, title: md.renderer.renderInlineAsText(image.children || [], md.options, {}) };
    } else if (children[0]?.type === 'link_open' && children.at(-1)?.type === 'link_close' && children.filter(t => t.type === 'link_open').length === 1 && !children.some(t => t.type === 'image')) {
      const source = children[0].attrGet('href') || '';
      if (isVideoSource(source)) candidate = { source, title: md.renderer.renderInlineAsText(children.slice(1, -1), md.options, {}) };
    } else if (children.length === 1 && children[0].type === 'text') {
      const source = children[0].content.trim();
      if (/^https?:\/\/\S+$/i.test(source) && isVideoSource(source)) candidate = { source };
    }
    if (candidate) {
      register(candidate, token);
      tokens[i + 1].type = 'video_consumed';
      tokens[i + 2].type = 'video_consumed';
    }
  }
  const wrap = (tag, style, text) => `<${tag} style="${escape(style)}">${text}</${tag}>`;
  md.renderer.rules.video_consumed = () => '';
  md.renderer.rules.video_placeholder = (tokens, i) => {
    const video = tokens[i].meta.video;
    const source = video.href ? `<a href="${escape(video.href)}" style="${escape(styles.videoSource)}">${escape(video.source)}</a>` : escape(video.source);
    return `<section data-video-id="${video.id}" style="${escape(styles.videoCard)}">`
      + wrap('p', styles.videoLabel, `在此插入视频 · ${video.number}`)
      + wrap('p', styles.videoTitle, escape(video.title))
      + wrap('p', styles.videoNote, '粘贴到公众号后，在这里插入视频，再删除整张占位卡。')
      + wrap('p', styles.videoSource, `来源：${source}`)
      + '</section>\n';
  };
  return videos;
}

export function videoChecklist(videos) {
  const lines = ['# 公众号视频待插入清单', '', '正文中保留了视频占位卡。请在公众号后台逐个插入视频，完成后删除对应的整张占位卡。', '此清单不代表视频已上传、可播放或已发布。', ''];
  if (!videos.length) lines.push('没有识别到需要插入的视频。', '');
  for (const video of videos) {
    lines.push(`## 视频 ${video.number}：${markdownEscape(video.title)}`, '',
      `- 位置：${markdownEscape(video.section)}${video.sourceLine ? `（原文第 ${video.sourceLine} 行）` : ''}`,
      `- 来源：${markdownEscape(video.source)}`,
      `- 类型：${video.kind === 'local' ? '本地文件' : video.kind === 'remote' ? '远程链接（未验证播放或后台支持）' : '手动定位信息'}`,
      ...(video.kind === 'local' ? [`- 本地文件：${video.exists ? '存在' : '未找到，请先补齐文件或修正路径'}`] : []),
      '- [ ] 在公众号后台对应位置插入视频', '- [ ] 删除对应的整张占位卡', '- [ ] 在公众号预览中确认位置和播放正常', '');
  }
  return lines.join('\n');
}
