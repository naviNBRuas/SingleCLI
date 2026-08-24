# Premium Web: Scroll-Driven Design

Patterns for high-end scroll work: Lenis smooth scrolling, ScrollTrigger
pinning, horizontal galleries, parallax depth layers — plus the
performance rules that keep everything at a steady 60fps.

## Lenis Smooth Scroll Setup

Lenis intercepts wheel/touch input and lerps the scroll position toward
a target, producing inertial scrolling while the page stays in normal
document flow. Drive it from GSAP's ticker so smooth scroll and
ScrollTrigger share a single clock.

```js
import Lenis from 'lenis';
import { gsap } from 'gsap';
import { ScrollTrigger } from 'gsap/ScrollTrigger';

gsap.registerPlugin(ScrollTrigger);

const lenis = new Lenis({
  duration: 1.2,
  easing: (t) => Math.min(1, 1.001 - Math.pow(2, -10 * t)),
  smoothWheel: true,
});

gsap.ticker.add((time) => lenis.raf(time * 1000));
gsap.ticker.lagSmoothing(0);
lenis.on('scroll', ScrollTrigger.update);
```

- Run exactly one rAF driver — never Lenis's internal loop beside
  `gsap.ticker`.
- Route anchor links through `lenis.scrollTo(target, { offset })`.
- Fall back to native scroll when `prefers-reduced-motion: reduce`.

## ScrollTrigger Pinning

Pinning freezes a section in the viewport while the scrollbar keeps moving — the backbone of scroll-told stories.

```js
gsap.timeline({
  scrollTrigger: {
    trigger: '.story',
    start: 'top top',
    end: '+=150%',
    pin: true,
    scrub: true,
    anticipatePin: 1,
  },
})
  .from('.step--1', { opacity: 0, y: 40 })
  .to('.step--2', { opacity: 1 });
```

- `pinSpacing: true` (default) reserves space so following content does
  not overlap the pinned element; `false` only for overlays.
- `pinType: 'fixed'` suits body-scrolled pages; `'transform'` when the
  scroller lives inside a transformed wrapper (common with Lenis).
- Call `ScrollTrigger.refresh()` after fonts and above-the-fold images
  load — late layout shifts misalign start/end positions.
- Pin outermost sections only; nested pins fight each other.

## Horizontal Scroll Sections

Translate a wide track sideways while its parent stays pinned.

```js
const track = document.querySelector('.h-track');

gsap.to(track, {
  x: () => -(track.scrollWidth - innerWidth),
  ease: 'none',
  scrollTrigger: {
    trigger: '.horizontal',
    start: 'top top',
    end: () => '+=' + (track.scrollWidth - innerWidth),
    pin: true,
    scrub: 1,
    invalidateOnRefresh: true,
  },
});
```

- Function-based values plus `invalidateOnRefresh` keep distances
  resize-safe; hardcoded pixels break on rotation.
- With Lenis already smoothing input, prefer `scrub: true` — stacking
  two smoothers feels detached and laggy.
- Uniform panel widths keep the math trivial; variable widths need a
  cumulative offset map.

## Parallax Depth Layers

Tag layers with `data-depth` and translate them at different rates as their section crosses the viewport.

```js
gsap.utils.toArray('[data-depth]').forEach((layer) => {
  const d = parseFloat(layer.dataset.depth);
  gsap.fromTo(layer, { yPercent: -8 * d }, {
    yPercent: 8 * d,
    ease: 'none',
    scrollTrigger: {
      trigger: layer.closest('[data-parallax]'),
      start: 'top bottom',
      end: 'bottom top',
      scrub: true,
    },
  });
});
```

- Animate `transform` only; scale layers ~1.05 so shifted edges never
  expose background gaps.
- Cap depth factors (roughly 0.2–1.5); extremes read as broken layout.
- Skip `background-attachment: fixed` on mobile — iOS Safari ignores it.

## Performance Pitfalls

### Scroll-jank causes

- Animating layout properties (`top`, `left`, `width`) instead of
  `transform`/`opacity`: every frame runs style → layout → paint.
- Interleaving DOM reads and writes inside scroll handlers thrashes
  layout; batch all reads, then perform writes once.
- Heavy unthrottled listener work — push it into rAF or ScrollTrigger
  callbacks, which already fire on frame boundaries.
- Oversized composited surfaces and full-screen `backdrop-filter`/blur
  exhaust the raster budget on integrated GPUs.

### will-change usage

- `will-change: transform` promotes an element to its own compositor
  layer; each layer costs GPU memory, so use it sparingly.
- Add it just before an animation starts and remove it when it ends —
  persistent promotion across many nodes degrades rendering.
- Engines often auto-promote during transform animations; confirm in
  the Layers panel before adding it manually.

## Launch checklist

- [ ] Lenis driven by `gsap.ticker`, `lagSmoothing(0)` set
- [ ] `ScrollTrigger.update` bound to Lenis scroll events
- [ ] `refresh()` after load and on resize
- [ ] Only `transform` and `opacity` animated
- [ ] `will-change` applied just-in-time and removed after
- [ ] Reduced-motion fallback: native scroll, static layers
