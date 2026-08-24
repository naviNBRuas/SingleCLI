# Parallax Section

Multi-layer parallax scrolling for hero and feature sections: background,
midground, and foreground layers translate at different rates while the user
scrolls, producing depth without heavy assets or canvas work.

## Pattern overview

Three stacked layers inside a fixed-height section. Each layer is pinned to a
GSAP ScrollTrigger timeline driven by `scrub`, so layer position stays locked
to scroll progress rather than running on an independent rAF loop.

- **Background** — slowest (`yPercent: 20`). Usually an image or gradient.
- **Midground** — moderate (`yPercent: -10`). Decorative shapes / imagery.
- **Foreground** — fastest (`yPercent: -35`). Primary content copy.

## Implementation sketch

```html
<section class="parallax" data-parallax>
  <div class="parallax__layer parallax__layer--bg" data-depth="0.2">
    <img src="sky.jpg" alt="" loading="lazy" decoding="async" />
  </div>
  <div class="parallax__layer parallax__layer--mid" data-depth="0.5"></div>
  <div class="parallax__layer parallax__layer--fg">
    <h1>Headline</h1>
    <p>Supporting copy.</p>
  </div>
</section>
```

```css
.parallax {
  position: relative;
  height: 100vh;
  overflow: hidden;
}

.parallax__layer {
  position: absolute;
  inset: 0;
  will-change: transform;
}

.parallax__layer--bg img {
  width: 100%;
  height: 120%; /* oversize so slow drift never reveals edges */
  object-fit: cover;
}
```

```js
import { gsap } from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";

gsap.registerPlugin(ScrollTrigger);

export function initParallax(root = document) {
  const reduceMotion = window.matchMedia(
    "(prefers-reduced-motion: reduce)"
  ).matches;
  const coarsePointer = window.matchMedia("(pointer: coarse)").matches;

  root.querySelectorAll("[data-parallax]").forEach((section) => {
    if (reduceMotion || coarsePointer || !window.ScrollTrigger) return;

    const layers = section.querySelectorAll("[data-depth]");
    const tweens = Array.from(layers).map((layer) =>
      gsap.to(layer, {
        yPercent: () => -100 * parseFloat(layer.dataset.depth),
        ease: "none",
        scrollTrigger: {
          trigger: section,
          start: "top bottom",
          end: "bottom top",
          scrub: true, // ties progress to scrollbar, no independent ticker
          invalidateOnRefresh: true,
        },
      })
    );

    // Cleanup hook — call when unmounting SPA views.
    section._parallaxCleanup = () => tweens.forEach((t) => t.scrollTrigger?.kill());
  });
}
```

## Usage

1. Add `[data-parallax]` to the section and `[data-depth]` (0–1) to each
   moving layer; higher depth = faster movement.
2. Call `initParallax()` once after DOM ready. For SPAs, call the stored
   `_parallaxCleanup()` on route teardown to kill ScrollTriggers.
3. Oversize background media ~20% vertically so the slowest layer never
   exposes its container edge mid-scroll.
4. Keep foreground content outside any transformed wrapper if you rely on
   `position: sticky` children — transforms create new containing blocks.

## Mobile fallback

Parallax on touch devices fights native momentum scrolling and burns battery.
This implementation disables it entirely when either guard matches:

- `prefers-reduced-motion: reduce` — accessibility requirement first.
- `(pointer: coarse)` — phones/tablets get static layered composition.

Layers simply render stacked at rest positions; the visual hierarchy survives
because z-index ordering is unchanged. If a client insists on *some* motion on
mobile, gate to `min-width: 768px` + coarse-pointer devices only and halve all
depth values — never run scrubbed timelines during iOS rubber-banding.

## Performance notes

- **Transform-only.** Animate `transform` (`yPercent`) exclusively. Never
  animate `top`/`margin-top`/`background-position` — those trigger layout or
  paint every frame; transforms stay on the compositor thread.
- **One ScrollTrigger per section**, not per frame tick. `scrub: true`
  reuses the browser's own scroll event instead of a JS rAF loop.
- **`will-change: transform`** only on layers that actually move — applying
  it broadly wastes GPU memory. Remove it if the layer becomes static.
- **Avoid layout reads inside triggers.** Depth values are read once per
  refresh (`invalidateOnRefresh` handles resize), not per scroll frame.
- **Compress and lazy-load** background images; prefer modern formats
  (AVIF/WebP) since parallax backgrounds are typically the largest asset on
  the page.
- **Cap layer count at three.** Each additional composited layer costs GPU
  memory proportional to viewport size; beyond three layers the depth effect
  plateaus while jank risk climbs.
