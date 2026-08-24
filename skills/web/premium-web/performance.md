# Web Performance for Premium Animated & 3D Websites

High-end visuals and fast loads are not opposing goals — they are both
requirements for a premium experience. A site that stutters, flashes, or
shifts mid-animation reads as cheap regardless of how polished the art
direction is. This skill defines the performance discipline for every
premium animated or WebGL-heavy build.

## Core Web Vitals: Targets and Meaning

Treat these thresholds as hard budgets, not aspirations. Test on real
mid-tier hardware (a 4x CPU-throttled profile) over Fast 3G/4G, never
on your development machine.

| Metric | Full name                | Target    | Why premium sites miss it                        |
| ------ | ------------------------ | --------- | ------------------------------------------------ |
| LCP    | Largest Contentful Paint | ≤ 2.5s    | Hero videos, giant images, render-blocking fonts |
| CLS    | Cumulative Layout Shift  | < 0.1     | Late fonts, unsized media, injected banners      |
| INP    | Interaction to Next Paint| ≤ 200ms   | Long tasks from animation frames and JS handlers |
| TBT    | Total Blocking Time      | ≤ 200ms   | Heavy framework boot before first input          |

Additional internal targets:

- First Contentful Paint ≤ 1.8s on mobile.
- Time to Interactive ≤ 3.5s on mid-tier mobile.
- No single long task > 50ms during load; keep per-frame main-thread
  work under 8ms on desktop and under 6ms on mobile.
- Total JavaScript transferred ≤ 200KB compressed on first load.

**LCP strategy:** identify the hero element early, preload it
(`<link rel="preload" as="image">`), never lazy-load it, serve it in a
modern format, and prefer server-rendered HTML over client-mounting it.

**CLS strategy:** reserve space for everything — intrinsic
width/height on media, `aspect-ratio` CSS, metric-matched font
fallbacks, and skeleton containers for late-mounted UI. Animate only
`transform` and `opacity` so effects never trigger layout.

**INP/TBT strategy:** split long tasks (`scheduler.yield()` or chunked
`setTimeout`), push physics and particle math to Workers, and defer
non-critical hydration until idle.

## Lazy-Loading Heavy 3D and WebGL

Never ship Three.js, Babylon, GLTF/DRACO loaders, or post-processing
pipelines in the main bundle. Put them behind dynamic import so the
page paints instantly and the 3D layer arrives on demand.

```js
// Mount the scene only after first paint and only if warranted.
let cleanup;
async function mountScene(container) {
  const [{ createScene }, { GLTFLoader }] = await Promise.all([
    import("./scene.js"),
    import("three/examples/jsm/loaders/GLTFLoader.js"),
  ]);
  cleanup = createScene(container, new GLTFLoader());
}

if ("requestIdleCallback" in window) {
  requestIdleCallback(() => mountScene(el), { timeout: 2000 });
}
```

Rules of thumb:

- Gate on intent, not just scroll: hover/focus on the hero, pointer
  proximity, or intersection are good triggers.
- Show a lightweight poster (static image or CSS gradient) while the
  scene boots; cross-fade once the first real frame has rendered.
- Prefer Draco/Meshopt-compressed glTF; keep hero assets ≤ 1.5MB and
  secondary assets ≤ 500KB.
- Destroy scenes on route change: dispose geometries, materials,
  textures, and renderers, and revoke object URLs to prevent leaks.
- Respect `prefers-reduced-motion`: swap the live scene for a static
  render instead of pausing something half-initialized.
- Feature-detect WebGL2 and fall back cleanly; a black canvas is worse
  than no canvas.

## Image Optimization

- Serve AVIF first, WebP second, JPEG last via `<picture>`:

  ```html
  <picture>
    <source type="image/avif" sizes="(max-width: 768px) 100vw, 60vw"
            srcset="hero-480.avif 480w, hero-960.avif 960w, hero-1440.avif 1440w">
    <source type="image/webp" sizes="(max-width: 768px) 100vw, 60vw"
            srcset="hero-480.webp 480w, hero-960.webp 960w, hero-1440.webp 1440w">
    <img src="hero-960.jpg" alt="Product hero"
         width="1440" height="810" decoding="async">
  </picture>
  ```

- Always declare intrinsic dimensions (or `aspect-ratio`) to protect
  CLS.
- Only the LCP image gets `fetchpriority="high"`; everything below the
  fold gets `loading="lazy"` plus `decoding="async"`.
- Generate 400w / 800w / 1200w / 1600w variants per breakpoint; never
  ship one oversized asset to every device.
- For hero video backdrops: muted, autoplaying, looping,
  `playsinline`, H.264 ≤ 2MB for ≤ 8s clips, with a required poster
  frame, paused whenever scrolled offscreen.

## Font Loading Without Layout Shift

- Self-host WOFF2 only; drop all legacy font formats.
- Pair `font-display: swap` with a metric-matched fallback
  (`size-adjust`, `ascent-override`) so the swap causes minimal reflow.
- Subset aggressively: inline the critical Latin subset, load extended
  sets async. One variable font beats several static weights.
- Preload only the 1–2 fonts needed above the fold with a `rel="preload"`
  link (`as="font"`, `type="font/woff2"`, `crossorigin`).
- Cap total webfont weight at ~100KB. Icon fonts are banned — use
  inline SVGs.

## Mobile Performance Budget (Concrete)

Classify the device up front via `navigator.hardwareConcurrency <= 4`,
`navigator.deviceMemory <= 4` where available, effective connection
type, and `prefers-reduced-motion`; persist the tier for the session.

| Effect                     | Desktop                  | Mid-tier mobile     | Low-tier / Save-Data |
| -------------------------- | ------------------------ | ------------------- | -------------------- |
| Particle count             | 20k                      | 3–5k                | 0 (static texture)   |
| Post-processing            | Bloom + DOF              | Bloom only          | Off                  |
| Shadow maps                | PCF soft, 2048px         | Single 1024px map   | Disabled             |
| Pixel ratio cap            | min(devicePixelRatio, 2) | 1.75                | 1                    |
| Ambient CSS/canvas motion  | Full                     | Halved duration     | Static               |

Additional hard rules:

- Disable every ambient animation when `prefers-reduced-motion`
  matches.
- Honor `Save-Data` and `navigator.connection.saveData` by skipping
  the 3D layer entirely.
- Pause all rAF loops, videos, and WebGL scenes when
  `document.hidden` is true.
- Circuit breaker: if rolling-average FPS stays below 45 for 3s, step
  quality down one tier automatically and log the downgrade.

## Verification Checklist

- [ ] Lighthouse mobile ≥ 90 in all categories on the production build.
- [ ] LCP element confirmed via `PerformanceObserver`, not screenshots.
- [ ] No CLS from fonts or media during a full-page scroll capture.
- [ ] Bundle analysis confirms no 3D library in the main chunk.
- [ ] Tested on a real low-end Android, not just CPU-throttled desktop.
- [ ] Every quality tier exercised by forcing its device-class flag.
