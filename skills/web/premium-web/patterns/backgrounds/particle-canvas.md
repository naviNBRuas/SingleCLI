# Particle Canvas Background

Reusable, dependency-free 2D-canvas particle background: drifting dots linked by
proximity lines ("constellation" effect). A cheaper alternative to a full Three.js
particle field for sections that don't need real 3D.

## When to use this instead of Three.js

- Decorative drift + proximity lines is enough — no camera moves, no true depth.
- Mobile-heavy traffic or tight JS budget (Three.js minifies to ~150 KB).
- Reach for Three.js only for real parallax, meshes-as-particles, or GPU-driven
  counts far above a few hundred.

## Implementation sketch

```js
// particle-canvas.js — vanilla ES module, zero dependencies
export function initParticleCanvas(canvas, {
  density = 0.00008,  // particles per CSS px²
  maxCount = 80,      // HARD cap — density alone balloons on big screens
  linkDist = 130,     // px — connect pairs closer than this
  speed = 0.4,
} = {}) {
  const ctx = canvas.getContext('2d');
  let raf = null, running = false, pts = [], W = 0, H = 0;
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  function resize() {
    W = canvas.clientWidth; H = canvas.clientHeight;
    canvas.width = W * dpr; canvas.height = H * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const n = Math.min(maxCount, Math.round(W * H * density));
    pts = Array.from({ length: n }, () => ({
      x: Math.random() * W, y: Math.random() * H,
      vx: (Math.random() - .5) * speed, vy: (Math.random() - .5) * speed,
    }));
  }
  function tick() {
    if (!running) return;
    raf = requestAnimationFrame(tick);
    ctx.clearRect(0, 0, W, H);
    for (const p of pts) {          // wrap edges → uniform density, no bounce math
      p.x += p.vx; p.y += p.vy;
      if (p.x < 0) p.x = W; else if (p.x > W) p.x = 0;
      if (p.y < 0) p.y = H; else if (p.y > H) p.y = 0;
    }
    for (let i = 0; i < pts.length; i++) {    // O(n²) links — keep n small
      const a = pts[i];
      ctx.beginPath(); ctx.arc(a.x, a.y, 1.6, 0, Math.PI * 2);
      ctx.fillStyle = 'rgba(148,163,255,.8)'; ctx.fill();
      for (let j = i + 1; j < pts.length; j++) {
        const b = pts[j], dx = a.x - b.x, dy = a.y - b.y, d2 = dx * dx + dy * dy;
        if (d2 < linkDist * linkDist) {
          const t = (1 - Math.sqrt(d2) / linkDist).toFixed(2);
          ctx.strokeStyle = `rgba(148,163,255,${t})`;
          ctx.beginPath(); ctx.moveTo(a.x, a.y); ctx.lineTo(b.x, b.y); ctx.stroke();
        }
      }
    }
  }
  const start = () => { if (!running) { running = true; raf = requestAnimationFrame(tick); } };
  const stop = () => { running = false; cancelAnimationFrame(raf); };
  window.addEventListener('resize', resize, { passive: true });
  resize(); start();
  return { start, stop };
}
```

## Usage

```html
<section class="hero">
  <canvas class="particle-bg" aria-hidden="true"></canvas>
</section>
```

Give `.hero` `position: relative` and `.particle-bg` `position: absolute; inset: 0; width: 100%; height: 100%`, then:

```js
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
if (!reduceMotion) {
  const fx = initParticleCanvas(document.querySelector('.particle-bg'));
  document.addEventListener('visibilitychange', () =>
    document.hidden ? fx.stop() : fx.start());  // pause when tab hidden
}
```

## Reduced-motion fallback

Under `prefers-reduced-motion: reduce`, skip `initParticleCanvas` entirely and render
a static stand-in — one pre-drawn frame (never call `start()`) or a plain gradient.

## Performance notes

- **Hard-cap the count** (`maxCount`): the link pass is O(n²) — 2× particles ≈ 4×
  pair checks. Stay at ≤ 100; beyond that, move up to WebGL/Three.js.
- **Pause when the tab hides**: `visibilitychange` → `stop()`/`start()` — rAF
  throttles in background tabs anyway, but explicit stops save battery.
- **Cap DPR at 2**, and measure only in `resize()` — never read layout in `tick()`.
- **Gate by viewport**: on long pages, add an `IntersectionObserver` so off-screen canvases idle.
