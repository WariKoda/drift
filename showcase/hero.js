// Draws the drift icon as an ASCII density field in a current that flows from left
// to right. Replaces the static hero image once it runs; without JavaScript or a
// 2D canvas the image stays. Colours come from the --ascii-* tokens in style.css.
(() => {
  const art = document.querySelector('.hero-art');
  const canvas = art?.querySelector('canvas');
  const ctx = canvas?.getContext('2d');
  if (!ctx) return;

  const root = document.documentElement;
  const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
  const systemDark = window.matchMedia('(prefers-color-scheme: dark)');
  const FRAME_MS = 1000 / 30;
  const STILL_TIME = 3;
  const RAMP_ICON = ':-=+*#%@';
  const RAMP_FIELD = ' .·:-=';
  const TRAIL = '»=-·';

  // The icon, 10 x 5 pixels: 1 = mint (the local blocks), 2 = purple (the d).
  const ICON = [
    '          ████████',
    '    ████  ████  ████',
    '          ████  ████',
    '████████  ████  ████',
    '          ████████',
  ];
  const iconPixel = (c, r) => (ICON[r][c * 2] === '█' ? (c < 5 ? 1 : 2) : 0);

  // Palette slots; index 0 means an empty cell.
  const TOKENS = ['', 'mint-1', 'mint-2', 'mint-3', 'purple-1', 'purple-2', 'purple-3', 'dim', 'dim-strong', 'spark', 'spark-inner'];
  const MINT = [1, 2, 3];
  const PURPLE = [4, 5, 6];
  const DIM = 7, DIM_STRONG = 8, SPARK = 9, SPARK_INNER = 10;

  let palette = [];
  let weight = '400';
  let font = '';
  let grid = null;
  let pointer = null;
  let particles = [];
  let nextParticle = 0;
  let particleSeq = 0;
  let visible = true;
  let frameId = 0;
  let lastFrame = 0;
  const start = performance.now();

  function hash(a, b) {
    let h = Math.imul(a | 0, 374761393) ^ Math.imul(b | 0, 668265263);
    h = Math.imul(h ^ (h >>> 13), 1103515245);
    h ^= h >>> 16;
    return (h >>> 0) / 4294967296;
  }

  const clamp = (v, min, max) => Math.max(min, Math.min(max, v));

  function readPalette() {
    const style = getComputedStyle(root);
    palette = TOKENS.map(name => (name ? style.getPropertyValue('--ascii-' + name).trim() : ''));
    weight = style.getPropertyValue('--ascii-weight').trim() || '400';
    if (grid) font = `${weight} ${grid.size}px ${style.getPropertyValue('--mono')}`;
  }

  function layout() {
    const width = canvas.clientWidth, height = canvas.clientHeight;
    if (!width || !height) return false;
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);

    const size = clamp(width / 62, 8, 11.5);
    font = `${weight} ${size}px ${getComputedStyle(root).getPropertyValue('--mono')}`;
    ctx.font = font;
    const cw = ctx.measureText('M').width;
    const lh = size * 1.2;
    const cols = Math.floor(width / cw), rows = Math.floor(height / lh);

    // Square icon pixels, about 58 % of the canvas height.
    const pxH = Math.max(2, Math.floor(rows * 0.58 / 5));
    let pxW = Math.max(2, Math.round(pxH * lh / cw));
    if (pxW * 10 > cols - 4) pxW = Math.floor((cols - 4) / 10);

    grid = {
      width, height, size, cw, lh, cols, rows, pxW, pxH,
      ox: (width - cols * cw) / 2,
      oy: (height - rows * lh) / 2,
      mx: Math.floor((cols - 10 * pxW) / 2),
      my: Math.floor((rows - 5 * pxH) / 2),
      glyphs: new Array(cols * rows),
      colors: new Uint8Array(cols * rows),
    };
    particles = [];
    nextParticle = 0;
    return true;
  }

  function iconAt(x, y) {
    const c = Math.floor((x - grid.mx) / grid.pxW), r = Math.floor((y - grid.my) / grid.pxH);
    return c < 0 || r < 0 || c >= 10 || r >= 5 ? 0 : iconPixel(c, r);
  }

  function put(x, y, glyph, color) {
    if (x < 0 || y < 0 || x >= grid.cols || y >= grid.rows) return;
    const i = y * grid.cols + x;
    grid.glyphs[i] = glyph;
    grid.colors[i] = color;
  }

  function compose(t) {
    const { cols, rows, cw, lh } = grid;
    const aspect = lh / cw;
    grid.colors.fill(0);

    for (let y = 0; y < rows; y++) {
      for (let x = 0; x < cols; x++) {
        // The pointer sends out ripples that also bend the icon.
        let ripple = 0, sx = x, sy = y;
        if (pointer) {
          const dx = x - pointer.x, dy = (y - pointer.y) * aspect, d = Math.hypot(dx, dy);
          if (d < 20) {
            ripple = Math.sin(d * 0.55 - t * 7) * (1 - d / 20);
            sx += (dx / (d + 0.01)) * ripple * 2.2;
            sy += (dy / (d + 0.01)) * ripple * 2.2 / aspect;
          }
        }
        const flow = 0.5 + 0.5 * Math.sin(
          x * 0.17 - t * 2.3 + Math.sin(y * 0.33 + t * 0.55) * 2.1 + Math.sin(x * 0.045 + y * 0.12 - t * 0.4) * 1.6,
        );
        const pixel = iconAt(sx, sy);
        if (pixel) {
          const v = clamp(0.35 + 0.65 * flow + ripple * 0.25, 0, 0.999);
          const shade = v > 0.78 ? 0 : v > 0.5 ? 1 : 2;
          put(x, y, RAMP_ICON[Math.floor(v * RAMP_ICON.length)], (pixel === 1 ? MINT : PURPLE)[shade]);
        } else {
          const falloff = 1 - clamp(Math.hypot(x / cols - 0.5, y / rows - 0.5) * 1.5, 0, 1);
          const v = clamp(flow ** 3 * falloff * 1.25 + Math.abs(ripple) * 0.45, 0, 0.999);
          if (v >= 0.2) put(x, y, RAMP_FIELD[Math.floor(v * RAMP_FIELD.length)], v > 0.6 ? DIM_STRONG : DIM);
        }
      }
    }

    // Sparks run through the mint rows and end at the d.
    if (t >= nextParticle) {
      const n = particleSeq++;
      const row = hash(n, 3) < 0.5 ? 1 : 3;
      particles.push({ y: grid.my + row * grid.pxH + Math.floor(hash(n, 4) * grid.pxH), speed: 26 + hash(n, 5) * 22, born: t });
      nextParticle = t + 0.09 + hash(n, 6) * 0.12;
    }
    const end = grid.mx + 5 * grid.pxW;
    particles = particles.filter(p => {
      const head = Math.floor(-2 + (t - p.born) * p.speed);
      if (head >= end) return false;
      for (let i = 0; i < TRAIL.length; i++) {
        const inMint = iconAt(head - i, p.y) === 1;
        put(head - i, p.y, TRAIL[i], i === 0 ? (inMint ? SPARK_INNER : SPARK) : (inMint ? MINT[0] : DIM_STRONG));
      }
      return true;
    });
  }

  function paint() {
    const { cols, cw, lh, ox, oy, glyphs, colors } = grid;
    ctx.clearRect(0, 0, grid.width, grid.height);
    ctx.font = font;
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    for (let color = 1; color < palette.length; color++) {
      ctx.fillStyle = palette[color];
      for (let i = 0; i < colors.length; i++) {
        if (colors[i] !== color) continue;
        ctx.fillText(glyphs[i], ox + (i % cols + 0.5) * cw, oy + (((i / cols) | 0) + 0.5) * lh);
      }
    }
  }

  function draw(now) {
    if (!grid) return;
    compose(reduceMotion.matches ? STILL_TIME : (now - start) / 1000);
    paint();
  }

  function tick(now) {
    frameId = requestAnimationFrame(tick);
    if (now - lastFrame < FRAME_MS) return;
    lastFrame = now;
    draw(now);
  }

  function schedule() {
    cancelAnimationFrame(frameId);
    frameId = 0;
    if (reduceMotion.matches || !visible) draw(performance.now());
    else frameId = requestAnimationFrame(tick);
  }

  function redrawColors() {
    readPalette();
    draw(performance.now());
  }

  canvas.hidden = false;
  art.classList.add('animated');
  readPalette();
  layout();

  canvas.addEventListener('pointermove', event => {
    if (!grid) return;
    const box = canvas.getBoundingClientRect();
    pointer = { x: (event.clientX - box.left - grid.ox) / grid.cw, y: (event.clientY - box.top - grid.oy) / grid.lh };
  });
  canvas.addEventListener('pointerleave', () => { pointer = null; });
  new ResizeObserver(() => { if (layout()) draw(performance.now()); }).observe(canvas);
  new IntersectionObserver(entries => {
    visible = entries[entries.length - 1].isIntersecting;
    schedule();
  }).observe(canvas);
  new MutationObserver(redrawColors).observe(root, { attributes: true, attributeFilter: ['data-theme'] });
  systemDark.addEventListener('change', redrawColors);
  reduceMotion.addEventListener('change', schedule);
  schedule();
})();
