# Animated Mesh Blob Background

Soft organic blobs that slowly morph and drift behind page content. Pure CSS — no canvas, no WebGL, no JavaScript. A cheap decorative alternative to mesh-gradient video or shader backgrounds; well suited to heroes, auth cards, and empty states.

## How it works

- An oversized element uses an animated 8-value `border-radius`, so its silhouette morphs organically.
- Layered radial gradients plus slow rotation produce the "mesh" color wash.
- Only `transform`, `border-radius`, and `opacity` ever animate, so frames stay on the compositor.
- A single `filter: blur()` on the container melts everything into soft light.

## Implementation sketch

```html
<section class="hero">
  <div class="blob-bg" aria-hidden="true">
    <div class="blob blob--one"></div>
    <div class="blob blob--two"></div>
  </div>
</section>
```

```css
.hero { position: relative; isolation: isolate; overflow: hidden; }

.blob-bg {
  position: absolute;
  inset: 0;
  z-index: -1;
  filter: blur(60px);
  opacity: 0.55;
}

.blob { position: absolute; width: 42vmax; height: 42vmax; }

.blob--one {
  top: -10%;
  left: -10%;
  background: radial-gradient(circle at 30% 30%, #7f5af0, transparent 70%);
  border-radius: 42% 58% 63% 37% / 45% 42% 58% 55%;
  animation: morph 18s ease-in-out infinite alternate, drift-a 26s ease-in-out infinite alternate;
}

.blob--two {
  bottom: -15%;
  right: -12%;
  background: radial-gradient(circle at 70% 40%, #2cb67d, transparent 70%);
  border-radius: 60% 40% 34% 66% / 56% 62% 38% 44%;
  animation: morph 22s ease-in-out infinite alternate-reverse, drift-b 31s ease-in-out infinite alternate;
}

@keyframes morph {
  0%   { border-radius: 42% 58% 63% 37% / 45% 42% 58% 55%; }
  50%  { border-radius: 58% 42% 37% 63% / 52% 55% 45% 48%; }
  100% { border-radius: 37% 63% 56% 44% / 49% 38% 62% 51%; }
}

@keyframes drift-a {
  from { transform: translate3d(0, 0, 0) rotate(0deg); }
  to   { transform: translate3d(8vw, 6vh, 0) rotate(40deg); }
}

@keyframes drift-b {
  from { transform: translate3d(0, 0, 0) scale(1); }
  to   { transform: translate3d(-7vw, -5vh, 0) scale(1.15) rotate(-30deg); }
}
```

## Usage

1. Put the markup as the first child of a `position: relative` section; `z-index: -1` keeps blobs behind content and out of pointer hit-testing.
2. Size blobs in `vmax` so they track the viewport with no media queries; tune blur between 40–80px (higher is softer, slightly pricier).

## Performance notes

- Compositor-friendly: animating only `transform`, `border-radius`, and `opacity` means no layout or paint work per frame.
- `translate3d()` pre-promotes each blob to its own GPU layer, avoiding layer churn when animations start.
- The static `blur()` is the biggest cost; shrink or remove it if low-end devices jank.
- Keep durations long (16s+) — slow motion hides dropped frames and reads as deliberate; two blobs is plenty.

## Reduced-motion fallback

```css
@media (prefers-reduced-motion: reduce) {
  .blob { animation: none; }
  .blob--one { transform: translate3d(2vw, 2vh, 0) rotate(20deg); }
  .blob--two { transform: scale(1.08); }
}
```

Static shapes preserve the same color wash and depth, just without any movement.
