// Xiaohongshu cards: lays a note's blocks (XhsDeck.swift) out as 3:4 or 1:1
// cards in one of the templates (design boards ㊿, 51), starting with the
// note's first words (the title is the post's, not a card's), and decides
// where the cards break (board 52): every way of breaking a section is scored, and the
// one that fills each card evenly, keeps each image with the sentence that
// leads into it and never strands a heading or a single line wins.
//
// Swift calls FumiXhs.layout(input) and gets back the cards drawn into
// #deck. In "export" mode they are full size, one under another, and their
// rects are returned for snapshots.

(() => {
  'use strict';

  const CARD_W = 300;
  /** Each card aims to be filled this much; emptier or fuller costs. */
  const TARGET_FILL = 0.75;
  /** Images may shrink this far to stay on the card with their text. */
  const MIN_IMAGE_SCALE = 0.85;
  /** Below this a card's content is centered (Memo and Grid). */
  const CENTER_BELOW = 0.55;

  // ───────── Text ─────────

  const CJK = /[⺀-⿿぀-ヿ㐀-䶿一-鿿豈-﫿]/;
  const LATIN = /[A-Za-z0-9]/;
  const cjkThenLatin = /([⺀-⿿぀-ヿ㐀-䶿一-鿿豈-﫿])([A-Za-z0-9])/g;
  const latinThenCjk = /([A-Za-z0-9%])([⺀-⿿぀-ヿ㐀-䶿一-鿿豈-﫿])/g;

  const escape = (text) => String(text).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
  const plain = (html) => String(html).replace(/<[^>]+>/g, '').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&quot;/g, '"').replace(/&amp;/g, '&');

  /** A space wherever Chinese meets a Latin letter or digit, across tags
   * too; the space goes outside the tag, so no highlight runs under it. */
  function space(html) {
    const parts = String(html).split(/(<[^>]+>)/);
    let out = '';
    let last = '';
    let tagsStart = -1;
    let opening = false;
    for (const part of parts) {
      if (!part) continue;
      if (part[0] === '<') {
        if (tagsStart < 0) { tagsStart = out.length; opening = false; }
        if (part[1] !== '/') opening = true;
        out += part;
        continue;
      }
      let text = part.replace(cjkThenLatin, '$1 $2').replace(latinThenCjk, '$1 $2');
      const first = plain(text)[0] || '';
      const needs = last && ((CJK.test(last) && LATIN.test(first)) || (LATIN.test(last) && CJK.test(first)));
      if (needs) {
        if (tagsStart >= 0 && opening) out = out.slice(0, tagsStart) + ' ' + out.slice(tagsStart);
        else text = ' ' + text;
      }
      out += text;
      const visible = plain(text);
      if (visible) last = visible[visible.length - 1];
      tagsStart = -1;
    }
    return out;
  }

  /** Sentences of one paragraph back together: a space only between two
   * Latin ones. */
  function joinSentences(sentences) {
    return sentences.reduce((joined, sentence) => {
      if (!joined) return sentence;
      const end = plain(joined).slice(-1);
      const start = plain(sentence)[0] || '';
      return joined + (/[A-Za-z0-9.,;:!?)"']/.test(end) && /[A-Za-z0-9("']/.test(start) ? ' ' : '') + sentence;
    }, '');
  }

  // ───────── Input ─────────

  let input = null;
  let family = 'memo';
  let cardH = 400;

  function templateClass() {
    if (input.template === 'grid') return 't-grid';
    if (input.template === 'post' || input.template === 'postDark') return 't-post' + (input.template === 'postDark' ? ' dark' : '');
    return 't-memo';
  }

  const isChinese = (text) => CJK.test(text || '');

  // ───────── Card chrome ─────────

  function avatar() {
    const author = input.author || {};
    if (author.avatar) return `<span class="avatar"><img src="${escape(author.avatar)}" alt=""></span>`;
    const initial = (author.name || '').trim().charAt(0).toUpperCase();
    return `<span class="avatar">${escape(initial)}</span>`;
  }

  function who(page) {
    const author = input.author || {};
    const handle = author.handle ? `<span class="handle">${escape(author.handle)}</span>` : '';
    return `<div class="who">${avatar()}<span class="names"><span class="name">${escape(author.name || '')}</span>${handle}</span>${page ? `<span class="page">${page}</span>` : ''}</div>`;
  }

  function foot(page) {
    const handle = (input.author || {}).handle || '';
    if (!handle && !page) return '';
    return `<div class="foot"><span>${escape(handle)}</span><span class="page">${page || ''}</span></div>`;
  }

  /** "2 / 7" in Memo and Grid, "2/7" in a post. */
  function pageLabel(index, total) {
    if (!input.pageNumbers || !total) return '';
    return family === 'post' ? `${index}/${total}` : `${index} / ${total}`;
  }

  /** A content card around `body`; `runhead` names the section it continues. */
  function contentCard(body, { runhead = '', index = 0, total = 0, centered = false } = {}) {
    const page = pageLabel(index, total);
    // Every post after the first hangs from the one before it.
    const classes = `card ${templateClass()}${centered ? ' centered' : ''}${family === 'post' && index > 1 ? ' thread' : ''}`;
    let top = '';
    if (family === 'post') top = who(page);
    else if (family === 'grid') top = `<div class="head"><span class="num">${index || ''}</span>${runhead ? `<span class="runhead">${escape(runhead)}</span>` : ''}</div>`;
    else if (runhead) top = `<div class="runhead">${escape(runhead)}</div>`;
    const bottom = family === 'post' ? '' : foot(page);
    return `<div class="${classes}" style="--card-h:${cardH}px">${top}<div class="body">${body}</div>${bottom}</div>`;
  }

  // ───────── Items ─────────

  function chapterHTML(html) {
    const words = space(html);
    if (family === 'post') return `<div class="chapter"><h2>${words}</h2></div>`;
    if (family === 'grid') {
      return `<div class="chapter"><h2><span class="hl">${words}</span></h2>` +
        `<svg aria-hidden="true" width="120" height="12" viewBox="0 0 120 12"><path d="M3 8 C 22 3, 42 11, 62 6 S 100 4, 116 7" stroke="#2F6BFF" stroke-width="2.4" fill="none" stroke-linecap="round"/></svg></div>`;
    }
    return `<div class="chapter"><h2><span class="hl">${words}</span></h2><div class="rule"></div></div>`;
  }

  /** The box an image takes at `scale` of the card's text width. */
  function imageBox(src, scale, maxHeight) {
    const size = images.get(src);
    const width = bodyWidth() * scale;
    let height = size ? width * size.h / size.w : width * 0.6;
    // Too tall for a card of its own: smaller, never cropped.
    if (maxHeight && height > maxHeight) return { width: Math.round(width * maxHeight / height), height: Math.round(maxHeight) };
    return { width: Math.round(width), height: Math.round(height) };
  }

  function figureHTML(block, scale = 1, maxHeight = 0) {
    const box = imageBox(block.src, scale, maxHeight);
    const lead = block.lead ? `<p class="lead">${space(block.lead)}</p>` : '';
    const caption = block.caption ? `<figcaption>${family === 'grid' ? '↑ ' : ''}${space(block.caption)}</figcaption>` : '';
    // Still loading: a quiet box of a guessed size until the next layout.
    const pending = images.has(block.src) ? '' : ' pending';
    return `<figure>${lead}<div class="img${pending}" style="width:${box.width}px;height:${box.height}px"><img src="${escape(block.src)}" alt=""></div>${caption}</figure>`;
  }

  function itemHTML(item) {
    switch (item.k) {
      case 'chapter': return chapterHTML(item.html);
      case 'sub': return `<h3>${space(item.html)}</h3>`;
      case 'p': return `<p>${space(joinSentences(item.sentences))}</p>`;
      case 'list': {
        const tag = item.ordered ? 'ol' : 'ul';
        const start = item.ordered && item.start > 1 ? ` start="${item.start}"` : '';
        return `<${tag}${start}>${item.items.map((html) => `<li>${space(html)}</li>`).join('')}</${tag}>`;
      }
      case 'figure': return figureHTML(item.block, item.scale || 1, item.maxHeight || 0);
      case 'bq': return `<blockquote class="inline"><p>${space(item.html)}</p></blockquote>`;
      case 'code': return wrapZoom(`<pre><code>${escape(item.text)}</code></pre>`, item.zoom);
      case 'table': return wrapZoom(space(item.html), item.zoom);
    }
    return '';
  }

  /** Code and tables too tall for a card of their own are scaled down. */
  function wrapZoom(html, zoom) {
    if (!zoom || zoom >= 1) return html;
    return `<div style="height:${Math.round(zoom * 1000) / 10}%;"><div class="zoomed" style="transform:scale(${zoom});width:${100 / zoom}%">${html}</div></div>`;
  }

  // ───────── Measuring ─────────

  const measure = () => document.getElementById('measure');
  let probes = {};
  let cache = new Map();

  /** A card of each kind, laid out off screen, to read the room inside. */
  function probe(runhead) {
    const key = runhead ? 'run' : 'plain';
    if (probes[key]) return probes[key];
    const holder = document.createElement('div');
    holder.innerHTML = contentCard('', { runhead: runhead ? '续' : '', index: 9, total: 99 });
    measure().appendChild(holder);
    const body = holder.querySelector('.body');
    const style = getComputedStyle(body);
    probes[key] = { body, capacity: body.clientHeight, width: body.clientWidth, gap: parseFloat(style.rowGap) || 14 };
    return probes[key];
  }

  function bodyWidth() { return probe(false).width; }

  /** The height an item takes on a card. */
  function heightOf(item, key) {
    if (key && cache.has(key)) return cache.get(key);
    const body = probe(false).body;
    body.innerHTML = itemHTML(item);
    const height = body.firstElementChild ? body.firstElementChild.getBoundingClientRect().height : 0;
    body.innerHTML = '';
    if (key) cache.set(key, height);
    return height;
  }

  let lineHeight = 26;

  // ───────── Images ─────────

  /** Each image's size once loaded; null if it couldn't be. */
  const images = new Map();
  /** Images on their way, so a layout per keystroke doesn't load them again. */
  const loading = new Map();

  function loadImages(blocks) {
    const sources = [...new Set(blocks.filter((b) => b.kind === 'figure').map((b) => b.src))];
    return Promise.all(sources.map((src) => {
      if (images.has(src)) return null;
      if (!loading.has(src)) {
        loading.set(src, new Promise((resolve) => {
          const image = new Image();
          const done = (size) => { images.set(src, size); loading.delete(src); resolve(); };
          image.onload = () => done({ w: image.naturalWidth || 1, h: image.naturalHeight || 1 });
          image.onerror = () => done(null);
          image.src = src;
        }));
      }
      return loading.get(src);
    }));
  }

  /** Counts layouts, so a late one for an old note or old text is dropped. */
  let generation = 0;

  // ───────── Sections and runs ─────────

  /**
   * The blocks in sections (each ## starts one) and the sections in runs:
   * stretches between forced breaks (a --- or a quote card). A run is broken
   * into cards as a whole; the cards of a run continue each other.
   */
  function runsOf(blocks, warnings) {
    const runs = [];
    let section = null;
    let run = null;
    let paragraph = 0;
    const startRun = (continues) => {
      run = { atoms: [], heading: section ? section.heading : '', continues, ids: [] };
      runs.push(run);
    };
    for (const block of blocks) {
      if (block.kind === 'figure' && images.get(block.src) === null) {
        warnings.push(`Image couldn't load: ${block.src.replace(/^fumi-xhs:\/\/local/, '')}`);
        continue;
      }
      if (block.kind === 'heading' && block.level <= 2) {
        section = { heading: plain(block.html) };
        startRun(false);
        run.atoms.push({ t: 'chapter', html: block.html, keep: true, id: block.id });
        continue;
      }
      if (!run) startRun(false);
      if (block.kind === 'pageBreak') { startRun(!!run.heading || run.atoms.length > 0); continue; }
      if (block.kind === 'quote') {
        const quote = { kind: 'quote', html: block.html, id: block.id };
        if (quoteFits(quote)) {
          if (run.atoms.length) runs.push({ quote, heading: run.heading, ids: [block.id] });
          else runs.splice(runs.indexOf(run), 0, { quote, heading: run.heading, ids: [block.id] });
          startRun(true);
          continue;
        }
        run.atoms.push({ t: 'bq', html: block.html, id: block.id });
        continue;
      }
      switch (block.kind) {
        case 'heading': run.atoms.push({ t: 'sub', html: block.html, keep: true, id: block.id }); break;
        case 'paragraph':
          paragraph += 1;
          block.sentences.forEach((html, i) => run.atoms.push({ t: 's', p: paragraph, i, html, id: block.id }));
          break;
        case 'list':
          paragraph += 1;
          block.items.forEach((html, i) => run.atoms.push({ t: 'li', p: paragraph, i, html, ordered: !!block.ordered, id: block.id }));
          break;
        case 'figure': run.atoms.push({ t: 'figure', block, id: block.id }); break;
        case 'code': run.atoms.push({ t: 'code', text: block.text || '', id: block.id }); break;
        case 'table': run.atoms.push({ t: 'table', html: block.html || '', id: block.id }); break;
      }
    }
    return runs.filter((r) => r.quote || r.atoms.length);
  }

  function quoteFits(quote) {
    const holder = document.createElement('div');
    holder.innerHTML = quoteCard(quote, 0, 0);
    measure().appendChild(holder);
    const body = holder.querySelector('.body');
    const children = [...body.children];
    const gap = parseFloat(getComputedStyle(body).rowGap) || 0;
    const used = children.reduce((sum, child) => sum + child.getBoundingClientRect().height, 0) + gap * Math.max(0, children.length - 1);
    holder.remove();
    return used <= body.clientHeight + 1;
  }

  /** atoms[i..j) as the items a card shows: sentences of one paragraph
   * together, items of one list together. */
  function itemsOf(atoms, i, j) {
    const items = [];
    for (let a = i; a < j; a++) {
      const atom = atoms[a];
      const last = items[items.length - 1];
      if (atom.t === 's') {
        if (last && last.k === 'p' && last.p === atom.p) { last.sentences.push(atom.html); last.to = atom.i; }
        else items.push({ k: 'p', p: atom.p, from: atom.i, to: atom.i, sentences: [atom.html] });
      } else if (atom.t === 'li') {
        if (last && last.k === 'list' && last.p === atom.p) { last.items.push(atom.html); last.to = atom.i; }
        else items.push({ k: 'list', p: atom.p, from: atom.i, to: atom.i, start: atom.i + 1, ordered: atom.ordered, items: [atom.html] });
      } else if (atom.t === 'chapter') items.push({ k: 'chapter', html: atom.html });
      else if (atom.t === 'sub') items.push({ k: 'sub', html: atom.html });
      else if (atom.t === 'figure') items.push({ k: 'figure', block: atom.block, atom: a });
      else if (atom.t === 'bq') items.push({ k: 'bq', html: atom.html, atom: a });
      else if (atom.t === 'code') items.push({ k: 'code', text: atom.text, atom: a });
      else if (atom.t === 'table') items.push({ k: 'table', html: atom.html, atom: a });
    }
    return items;
  }

  function itemKey(item) {
    switch (item.k) {
      case 'p': return `p${item.p}:${item.from}-${item.to}`;
      case 'list': return `l${item.p}:${item.from}-${item.to}`;
      case 'chapter': return `c:${item.html}`;
      case 'sub': return `h:${item.html}`;
      default: return `a${item.atom}`;
    }
  }

  /** How tall atoms[i..j) stand on a card, and how much of it is image. */
  function measureRange(atoms, i, j, runKey) {
    const items = itemsOf(atoms, i, j);
    const gap = probe(false).gap;
    let height = gap * Math.max(0, items.length - 1);
    let image = 0;
    for (const item of items) {
      height += heightOf(item, `${runKey}/${itemKey(item)}`);
      if (item.k === 'figure') image += imageBox(item.block.src, 1).height;
    }
    return { items, height, image };
  }

  /**
   * The best way to break a run into cards (board 52): every card is scored
   * by how close it comes to TARGET_FILL, every break by where it falls.
   */
  function breakRun(run, runKey) {
    const atoms = run.atoms;
    const n = atoms.length;
    const capacityFirst = probe(run.continues && !!run.heading).capacity;
    const capacityNext = probe(!!run.heading).capacity;
    const best = new Array(n + 1).fill(Infinity);
    const from = new Array(n + 1).fill(-1);
    const layout = new Array(n + 1).fill(null);
    best[0] = 0;

    for (let j = 1; j <= n; j++) {
      // A heading never ends a card unless nothing follows it.
      if (atoms[j - 1].keep && j < n) continue;
      for (let i = j - 1; i >= 0; i--) {
        if (best[i] === Infinity) continue;
        const capacity = i === 0 ? capacityFirst : capacityNext;
        const { items, height, image } = measureRange(atoms, i, j, runKey);
        const shrinkable = image * (1 - MIN_IMAGE_SCALE);
        const single = j - i === 1 || (j - i === 2 && atoms[i].keep);
        let scale = 1;
        let oversize = false;
        if (height > capacity) {
          if (height - shrinkable <= capacity && image > 0) {
            scale = 1 - (height - capacity) / image;
          } else if (single) {
            oversize = true;
          } else {
            break; // Taking in more only makes it taller.
          }
        }
        const fill = Math.min(1, height / capacity);
        const under = Math.max(0, TARGET_FILL - fill);
        const over = Math.max(0, fill - 0.92);
        let cost = 100 * under * under + 40 * over * over + 6;
        if (scale < 1) cost += 4;
        if (oversize) cost += 2;
        cost += breakCost(atoms, i, j, runKey);
        if (best[i] + cost < best[j]) {
          best[j] = best[i] + cost;
          from[j] = i;
          layout[j] = { items, height, capacity, scale, oversize };
        }
      }
    }

    // Walk back from the end; fall back to one atom a card if nothing fit.
    const cards = [];
    let j = n;
    if (best[n] === Infinity) {
      for (let a = 0; a < n; a++) cards.push(cardFor(run, atoms, a, a + 1, runKey));
      return cards;
    }
    while (j > 0) {
      const i = from[j];
      cards.unshift({ ...layout[j], atoms: atoms.slice(i, j), first: i === 0 });
      j = i;
    }
    return cards;
  }

  function cardFor(run, atoms, i, j, runKey) {
    const capacity = probe(i === 0 ? run.continues && !!run.heading : !!run.heading).capacity;
    const { items, height } = measureRange(atoms, i, j, runKey);
    return { items, height, capacity, scale: 1, oversize: height > capacity, atoms: atoms.slice(i, j), first: i === 0 };
  }

  /** What breaking before atoms[j] costs, for a card that starts at atoms[i]. */
  function breakCost(atoms, i, j, runKey) {
    if (j >= atoms.length) return 0;
    const before = atoms[j - 1];
    const after = atoms[j];
    if (before.t === 's' && after.t === 's' && before.p === after.p) {
      let cost = 10;
      // A part of a paragraph alone on its card's last or first line.
      let start = j - 1;
      while (start > i && atoms[start - 1].t === 's' && atoms[start - 1].p === before.p) start--;
      let end = j;
      while (end + 1 < atoms.length && atoms[end + 1].t === 's' && atoms[end + 1].p === before.p) end++;
      const head = linesOf(atoms, start, j, runKey);
      const tail = linesOf(atoms, j, end + 1, runKey);
      if (head < 2 || tail < 2) cost += 30;
      return cost;
    }
    if (before.t === 'li' && after.t === 'li' && before.p === after.p) return 3;
    return 0;
  }

  function linesOf(atoms, i, j, runKey) {
    const item = itemsOf(atoms, i, j)[0];
    return Math.round(heightOf(item, `${runKey}/${itemKey(item)}`) / lineHeight);
  }

  // ───────── Cards ─────────

  function quoteCard(quote, index, total) {
    const mark = family === 'post' ? '' : '<div class="mark" aria-hidden="true">“</div>';
    const body = `${mark}<blockquote>${space(quote.html)}</blockquote>`;
    return contentCard(body, { index, total, centered: true }).replace('class="card ', 'class="card quote ');
  }

  function endCard(index, total) {
    const author = input.author || {};
    const line = input.endCard && input.endCard.line ? `<p class="line">${space(escape(input.endCard.line))}</p>` : '';
    const body = family === 'post'
      ? line || '<p class="line"></p>'
      : `<div class="name">${escape(author.name || '')}</div>${author.handle ? `<div class="handle">${escape(author.handle)}</div>` : ''}${line}`;
    return contentCard(body, { index, total, centered: true }).replace('class="card ', 'class="card end ');
  }

  // ───────── Layout ─────────

  async function layout(next) {
    input = next;
    family = input.template === 'grid' ? 'grid' : (input.template === 'post' || input.template === 'postDark') ? 'post' : 'memo';
    cardH = input.ratio === '1:1' ? CARD_W : 400;
    const exporting = input.mode === 'export';
    document.body.className = exporting ? 'export' : 'preview';
    if (input.accent) document.body.style.setProperty('--accent', input.accent);

    measure().innerHTML = '';
    probes = {};
    cache = new Map();
    const deck = input.deck;
    const warnings = [];
    try { await document.fonts.ready; } catch (e) { /* fonts load as they can */ }
    // Export waits for every image. The preview lays the cards out at once,
    // guessing the size of images still loading, and again when they arrive.
    const mine = ++generation;
    const arriving = loadImages(deck.blocks);
    if (exporting) {
      await arriving;
    } else if (deck.blocks.some((block) => block.kind === 'figure' && !images.has(block.src))) {
      const pendingInput = input;
      arriving.then(async () => {
        if (mine !== generation) return;
        const result = await layout(pendingInput);
        const handler = window.webkit && window.webkit.messageHandlers.xhs;
        if (handler && mine + 1 === generation) handler.postMessage({ type: 'layout', ...result });
      });
    }

    // One line of body text, for counting lines.
    const sample = probe(false).body;
    sample.innerHTML = '<p>字</p>';
    lineHeight = sample.firstElementChild.getBoundingClientRect().height || 26;
    sample.innerHTML = '';

    // Every run broken into cards.
    const plans = [];
    runsOf(deck.blocks, warnings).forEach((run, r) => {
      if (run.quote) { plans.push({ quote: run.quote, ids: run.ids }); return; }
      for (const card of breakRun(run, `r${r}`)) {
        const runhead = run.heading && !(card.first && !run.continues)
          ? `${run.heading} · ${isChinese(run.heading) ? '续' : 'cont.'}` : '';
        const fill = card.height / card.capacity;
        const hasChapter = card.items.some((item) => item.k === 'chapter');
        plans.push({
          card, runhead,
          centered: family !== 'post' && !hasChapter && fill < CENTER_BELOW,
          ids: [...new Set(card.atoms.map((atom) => atom.id))],
        });
      }
    });

    // No cover: the post's title is Xiaohongshu's own, and the cards start
    // with the note's first words.
    const withEnd = !!(input.endCard && input.endCard.on);
    const total = plans.length + (withEnd ? 1 : 0);
    const html = [];
    plans.forEach((plan, p) => {
      const index = p + 1;
      if (plan.quote) { html.push(quoteCard(plan.quote, index, total)); return; }
      const { card } = plan;
      const items = card.items.map((item) => {
        if (item.k === 'figure') {
          const rest = card.height - imageBox(item.block.src, 1).height;
          const maxHeight = card.oversize ? Math.max(60, card.capacity - rest) : 0;
          return { ...item, scale: card.scale, maxHeight };
        }
        if ((item.k === 'code' || item.k === 'table') && card.oversize) {
          return { ...item, zoom: Math.max(0.5, card.capacity / card.height) };
        }
        return item;
      });
      html.push(contentCard(items.map(itemHTML).join(''), { runhead: plan.runhead, index, total, centered: plan.centered }));
    });
    if (withEnd) html.push(endCard(total, total));

    const holder = document.getElementById('deck');
    if (exporting) {
      holder.innerHTML = html.join('');
    } else {
      const columns = 3;
      const width = Math.max(90, Math.floor((document.body.clientWidth - 44 - 8 * (columns - 1)) / columns));
      const scale = width / CARD_W;
      document.body.style.setProperty('--cell-w', `${width}px`);
      document.body.style.setProperty('--cell-h', `${Math.round(cardH * scale)}px`);
      document.body.style.setProperty('--scale', String(scale));
      holder.innerHTML = html.map((card, i) => `<div class="cell" data-index="${i}">${card}</div>`).join('');
    }
    measure().innerHTML = '';

    if (total > 18) {
      warnings.push(`${total} cards; Xiaohongshu takes 18 at most. Merge a few sections or leave some out.`);
    }

    cardIds = [...plans.map((plan) => plan.ids), ...(withEnd ? [[]] : [])];
    const result = { count: total, warnings };
    if (exporting) {
      await Promise.all([...holder.querySelectorAll('img')].map((img) => img.decode().catch(() => null)));
      result.rects = [...holder.querySelectorAll('.card')].map((card) => {
        const rect = card.getBoundingClientRect();
        return { x: rect.left + window.scrollX, y: rect.top + window.scrollY, width: rect.width, height: rect.height };
      });
    }
    // The preview's web view is made this tall; the pane scrolls it.
    result.height = holder.getBoundingClientRect().height;
    return result;
  }

  // ───────── Preview interaction ─────────

  let cardIds = [];

  document.addEventListener('contextmenu', (event) => {
    const cell = event.target.closest && event.target.closest('.cell');
    event.preventDefault();
    if (!cell || !window.webkit || !window.webkit.messageHandlers.xhs) return;
    const index = Number(cell.dataset.index);
    document.querySelectorAll('.cell.menu').forEach((other) => other.classList.remove('menu'));
    cell.classList.add('menu');
    window.webkit.messageHandlers.xhs.postMessage({
      type: 'menu', index, x: event.clientX, y: event.clientY, ids: cardIds[index] || [],
    });
  });

  window.FumiXhs = {
    layout,
    menuClosed() { document.querySelectorAll('.cell.menu').forEach((cell) => cell.classList.remove('menu')); },
  };
})();
