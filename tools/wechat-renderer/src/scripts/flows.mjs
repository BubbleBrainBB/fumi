import { parseYaml } from '../vendor/parsers.mjs';

const escape = value => String(value).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
const text = (value, name, max) => {
  if (typeof value !== 'string' || !value.trim() || [...value.trim()].length > max || /[\r\n\x00-\x1f]/.test(value)) {
    throw new Error(`wechat-flow 的 ${name} 必须是 1–${max} 字符的单行文字。`);
  }
  return value.trim();
};

// A fixed four-node message loop. Source controls labels, never SVG or CSS.
export function prepareFlows(md, tokens, { lineOffset }) {
  const flows = [];
  for (const token of tokens) {
    if (token.type !== 'fence' || token.info.trim() !== 'wechat-flow') continue;
    const data = parseYaml(token.content);
    if (!data || typeof data !== 'object' || Array.isArray(data) ||
      Object.keys(data).some(key => !['title', 'steps', 'returnLabel'].includes(key))) {
      throw new Error('wechat-flow 只接受 title、steps、returnLabel。');
    }
    if (!Array.isArray(data.steps) || data.steps.length !== 4) throw new Error('wechat-flow 的 steps 必须包含 4 项：手机、消息服务、Agent、电脑。');
    const flow = {
      id: `message-loop-${String(flows.length + 1).padStart(2, '0')}`,
      number: String(flows.length + 1).padStart(2, '0'),
      kind: 'message-loop', sourceLine: token.map[0] + lineOffset + 1,
      title: text(data.title, 'title', 24),
      steps: data.steps.map(label => text(label, 'steps', 10)),
      returnLabel: text(data.returnLabel, 'returnLabel', 24),
      durationSeconds: 10, wechatVerified: false,
    };
    flows.push(flow);
    token.type = 'message_loop';
    token.meta = flow;
  }
  md.renderer.rules.message_loop = (tokens, i) => renderFlow(tokens[i].meta);
  return flows;
}

function renderFlow(flow) {
  const { id, steps } = flow;
  const icons = [
    '<rect x="-11" y="-18" width="22" height="36" rx="4"/><path d="M-4 12h8M-3-13h6"/>',
    '<path d="M-18-14h36v23H-5l-10 9V9h-3zM-10-6h20M-10 1h12"/>',
    '<path d="M-10-10l-10 10 10 10M10-10L20 0 10 10M4-16l-8 32"/>',
    '<rect x="-21" y="-15" width="42" height="27" rx="3"/><path d="M0 12v8M-12 20h24"/>',
  ];
  const points = [[108, 92], [412, 92], [412, 292], [108, 292]];
  const nodes = points.map(([x, y], i) => `<g transform="translate(${x} ${y})">
    <circle r="43" fill="#202923" stroke="#57625a" stroke-width="1.5"/>
    <circle r="43" fill="none" stroke="#d6f36a" stroke-width="2.5" opacity="0"><animate attributeName="opacity" values="0;1;0;0;0" keyTimes="0;0.06;0.2;0.8;1" dur="10s" begin="${[0, 3, 5, 8][i]}s" repeatCount="indefinite"/></circle>
    <g fill="none" stroke="#e9eedf" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">${icons[i]}</g>
    <rect x="-108" y="54" width="216" height="30" fill="#17211b"/>
    <text y="75" fill="#f2f5ec" font-size="21" text-anchor="middle">${escape(steps[i])}</text>
    <text x="-51" y="-45" fill="#b1bba9" font-size="13">0${i + 1}</text>
  </g>`).join('');
  return `<section data-animation-id="${id}" style="margin:36px 0;background:#17211b;color:#f2f5ec;padding:24px 16px 18px;border-radius:2px;">
    <p style="margin:0 8px 12px;font-size:11px;line-height:1.5;letter-spacing:2px;color:#d6f36a;">FIG. ${flow.number} / MESSAGE LOOP</p>
    <p style="margin:0 8px 8px;font-size:21px;line-height:1.5;font-weight:650;color:#f2f5ec;">${escape(flow.title)}</p>
    <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 520 395" width="520" height="395" role="img" aria-labelledby="${id}-title ${id}-desc" style="display:block;width:100%;height:auto;overflow:visible;font-family:-apple-system,BlinkMacSystemFont,'PingFang SC','Microsoft YaHei',sans-serif;">
      <title id="${id}-title">${escape(flow.title)}</title>
      <desc id="${id}-desc">${steps.map(escape).join(' → ')}。${escape(flow.returnLabel)}。光点按此顺序循环移动。</desc>
      <path d="M108 92H412V292H108Z" fill="none" stroke="#566257" stroke-width="1.5"/>
      <path d="M252 87l8 5-8 5M407 186l5 8 5-8M268 287l-8 5 8 5M103 206l5-8 5 8" fill="none" stroke="#d6f36a" stroke-width="2"/>
      <text x="260" y="199" fill="#d6f36a" font-size="13" text-anchor="middle" letter-spacing="3">SEND → RUN</text>
      <text x="260" y="225" fill="#a7b3a4" font-size="13" text-anchor="middle" letter-spacing="3">RETURN ↺</text>
      <g data-flow-packet="true">
        <animateMotion path="M108 92H412V292H108Z" dur="10s" calcMode="linear" keyPoints="0;0.301587;0.5;0.801587;1" keyTimes="0;0.3;0.5;0.8;1" repeatCount="indefinite"/>
        <circle r="13" fill="#d6f36a" opacity="0.12"/><circle r="5" fill="#d6f36a"/>
      </g>
      ${nodes}
    </svg>
    <p style="margin:0 8px;padding-top:14px;border-top:1px solid #465247;color:#c0cbbb;font-size:13px;line-height:1.7;">↳ ${escape(flow.returnLabel)}</p>
  </section>\n`;
}
