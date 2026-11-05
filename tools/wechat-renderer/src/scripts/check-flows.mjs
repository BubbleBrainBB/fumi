import assert from 'node:assert/strict';
import { renderArticle, buildPreview } from './render.mjs';

const fence = '```wechat-flow\ntitle: 消息的往返\nsteps: [手机, 飞书, Codex, 电脑]\nreturnLabel: 结果返回手机\n```';
const source = `---\ntitle: 固定标题\n---\n# 固定标题\n\n${fence}\n\n## 原有标题\n\n原文。\n\n${fence}`;
const result = renderArticle(source, { theme: 'motion-journal' });
assert.deepEqual(renderArticle(source, { theme: 'motion-journal' }), result);
assert.equal(result.report.animations.length, 2);
assert.equal(result.report.animations[0].sourceLine, 6);
assert.equal(result.report.animations[1].sourceLine, 16);
assert.deepEqual(result.report.animations.map(f => f.id), ['message-loop-01', 'message-loop-02']);
assert(result.report.animations.every(f => f.wechatVerified === false));
assert.equal(result.report.images.length, 0);
assert.equal(result.report.sections, 1);
assert.equal(result.report.toc.enabled, false);
assert(result.html.includes('<animateMotion'));
assert(!/<script\b|<style\b|\sclass=/.test(result.html));
assert.equal((result.html.match(/data-theme-ornament="heading"/g) || []).length, 1);
assert.equal(renderArticle('普通正文', { theme: 'motion-journal' }).report.animations.length, 0);
assert(!renderArticle('普通正文', { theme: 'motion-journal' }).html.includes('<svg'));
const raw = renderArticle('<svg onload="alert(1)"></svg>\n\n```svg\n<svg></svg>\n```');
assert.equal(raw.report.animations.length, 0);
assert(!raw.html.includes('<svg'));
assert(raw.html.includes('&lt;svg'));
const code = renderArticle('````text\n' + fence + '\n````');
assert.equal(code.report.animations.length, 0);
assert(code.html.includes('wechat-flow'));
for (const bad of [
  fence.replace('title: 消息的往返', 'title: []'),
  fence.replace('steps: [手机, 飞书, Codex, 电脑]', 'steps: [手机, 飞书]'),
  fence.replace('returnLabel: 结果返回手机', 'returnLabel: 123'),
  fence.replace('title: 消息的往返', 'title: ' + '字'.repeat(25)),
  fence.replace('title: 消息的往返', 'title: "第一行\\n第二行"'),
  fence.replace('title: 消息的往返', 'css: 任意样式\ntitle: 消息的往返'),
]) assert.throws(() => renderArticle(bad), /wechat-flow/);
const escaped = renderArticle(fence.replace('title: 消息的往返', 'title: \'<script>&"\''));
assert(escaped.html.includes('&lt;script&gt;&amp;&quot;'));
assert(!escaped.html.includes('<script>'));
assert(!buildPreview(escaped).includes('<script>&"'));
const indexCard = renderArticle('```sh\necho hello\n  echo world\n```', { theme: 'motion-journal' });
assert(!indexCard.html.includes('●'));
assert(indexCard.html.includes('>SH</span>'));
assert(indexCard.html.includes('echo hello\n  echo world\n</code>'));
assert(renderArticle('```\nplain code\n```', { theme: 'motion-journal' }).html.includes('>CODE</span>'));
console.log('PASS: fixed flow schema, deterministic SVG, explicit-only conversion, source lines, escaping, motion theme and code window.');
