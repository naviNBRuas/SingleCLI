# Animated Gradient + Noise Background

Slow-drifting conic/radial gradient blobs layered under a subtle film-grain
overlay. The noise kills banding on large soft gradients and makes the motion
feel organic rather than "screensaver".

## When to use

- Hero sections, empty states, loaders, marketing surfaces.
- Dark UIs especially: big gradients band badly there, and grain hides it.

## Implementation

```html
<div class="bg-gradient" aria-hidden="true">
  <div class="blob blob-a"></div>
  <div class="blob blob-b"></div>
</div>
<svg class="bg-noise" aria-hidden="true">
  <filter id="grain">
    <feTurbulence type="fractalNoise" baseFrequency="0.8" numOctaves="2" stitchTiles="stitch" />
    <feColorMatrix type="saturate" values="0" />
  </filter>
  <rect width="100%" height="100%" filter="url(#grain)" />
</svg>
```

```css
.bg-gradient {
  position: fixed; inset: 0; z-index: -2; overflow: hidden;
  background: #0b0d12;
}
.blob {
  position: absolute;
  width: 60vmax; height: 60vmax;
  border-radius: 50%;
  filter: blur(90px); /* costly: stay in the 60-120px range */
  opacity: 0.55;
  will-change: transform;
}
.blob-a {
  top: -20%; left: -15%;
  background: conic-gradient(from 120deg, #6d5cff, #b84dff, #ff5c87, #6d5cff);
  animation: drift-a 26s ease-in-out infinite alternate;
}
.blob-b {
  bottom: -30%; right: -20%;
  background: radial-gradient(circle at 40% 40%, #00c2a8, transparent 65%);
  animation: drift-b 34s ease-in-out infinite alternate;
}
@keyframes drift-a {
  from { transform: translate3d(0, 0, 0) scale(1); }
  to   { transform: translate3d(12vw, 8vh, 0) scale(1.15); }
}
@keyframes drift-b {
  from { transform: translate3d(0, 0, 0) rotate(0deg); }
  to   { transform: translate3d(-10vw, -6vh, 0) rotate(25deg); }
}
.bg-noise {
  position: fixed; inset: 0; z-index: -1;
  width: 100%; height: 100%;
  opacity: 0.06;
  mix-blend-mode: overlay;
  pointer-events: none;
}
```

## Reduced-motion fallback

```css
@media (prefers-reduced-motion: reduce) {
  .blob { display: none; }
  .bg-gradient {
    background:
      radial-gradient(circle at 20% 20%, rgba(109, 92, 255, 0.35), transparent 60%),
      radial-gradient(circle at 80% 80%, rgba(0, 194, 168, 0.25), transparent 60%),
      #0b0d12;
  }
}
```

Reduced-motion users get one static composed gradient - atmosphere intact,
nothing animating. Drifting backgrounds are exactly what they opted out of.

## Performance notes

- Animate only `transform`/`opacity`; never tween `background-position` on
  full-viewport gradients or re-run filters per frame.
- Blur re-rasterizes every frame a blob moves: cap radii (~60-120px), keep
  2-3 blobs max, and use `translate3d` so each gets its own compositor layer.
- Do NOT redraw a full-viewport noise canvas every `requestAnimationFrame`
  tick - per-pixel regeneration at 1920x1080 x 60fps burns CPU and battery
  for shimmer nobody consciously perceives. Generate one static tile instead.
- If animated grain is truly needed, pre-render ~4 tiles once and cycle them
  with CSS `steps()` background swaps; no per-frame JS work.
- Render grain at 1x DPR and upscale; grain masks upscaling artifacts.
- JS variants must pause on `visibilitychange` (CSS animations auto-throttle
  in hidden tabs; raw rAF loops do not).

## Canvas grain tile (generate once, never per frame)

```js
const c = Object.assign(document.createElement("canvas"), { width: 128, height: 128 });
const ctx = c.getContext("2d");
const img = ctx.createImageData(128, 128);
for (let i = 0; i < img.data.length; i += 4) {
  const v = (Math.random() * 255) | 0;
  img.data.set([v, v, v, 255], i);
}
ctx.putImageData(img, 0, 0); // c.toDataURL() -> repeating bg image, opacity ~0.05
```

## Tuning

- Grain too strong: drop `.bg-noise` opacity toward 0.03-0.05.
- Banding survives: raise `baseFrequency` slightly or add a third blob.
- Feels busy: stretch durations toward 40s+.
- Mobile jank: shrink the blur, hide a blob under 768px, test on-device.
