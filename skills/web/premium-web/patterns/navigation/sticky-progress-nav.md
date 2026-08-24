# Sticky Progress Nav

A fixed top navigation bar that stays visible while scrolling and includes a
horizontal scroll-progress indicator showing how far the user has read.

## When to use

- Long-form content: articles, documentation, tutorials, case studies.
- Pages where orientation matters and users benefit from knowing their
  position relative to the end of the document.
- Marketing/landing pages where the CTA must remain reachable at all times.

## When NOT to use

- Short pages (< ~2 viewports of scroll) where progress is meaningless.
- Apps with their own persistent chrome (the bar competes with native UI).
- Content-first reading modes where every pixel of vertical space matters.

## Anatomy

```
┌──────────────────────────────────────────────┐
│  Logo   Link   Link   Link        [CTA]      │  ← sticky nav
│  ▓▓▓▓▓▓▓▓▓▓▓░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░░  │  ← progress track
└──────────────────────────────────────────────┘
```

1. **Nav container** — `position: sticky; top: 0` so it scrolls out only when
   its natural position ends (or `fixed` for always-on).
2. **Progress track** — full-width, subtle background (e.g. `rgba(0,0,0,.08)`),
   2–4px tall, anchored to the bottom edge of the nav.
3. **Progress fill** — accent-colored element whose width maps to scroll %.

## Implementation

Markup:

```html
<header class="sticky-nav">
  <nav aria-label="Primary">…links…</nav>
  <div class="progress" role="presentation">
    <div class="progress__fill"></div>
  </div>
</header>
```

Styles:

```css
.sticky-nav {
  position: sticky;
  top: 0;
  z-index: 100;
}

.progress__fill {
  height: 3px;
  width: 100%;
  transform-origin: 0 50%;
  transform: scaleX(0); /* updated via JS */
}
```

Script — prefer `transform: scaleX()` over animating `width`; transforms skip
layout and paint on the compositor:

```js
const fill = document.querySelector('.progress__fill');

function onScroll() {
  const doc = document.documentElement;
  const max = doc.scrollHeight - doc.clientHeight;
  const ratio = max > 0 ? Math.min(doc.scrollTop / max, 1) : 0;
  fill.style.transform = `scaleX(${ratio})`;
}

document.addEventListener('scroll', onScroll, { passive: true });
onScroll();
```

Wrap updates in `requestAnimationFrame` if you also read layout in the same
handler, and mark the listener `{ passive: true }` so scrolling never blocks.

## Accessibility

- Mark the progress bar `role="presentation"` or `aria-hidden="true"` — it is
  redundant decoration, not an operable widget.
- Keep focus styles on nav links visible against the sticky backdrop.
- Respect `prefers-reduced-motion`: the fill is fine (position-driven), but any
  transition/easing added to it must be removed or shortened.
- Ensure contrast between fill, track, and page background meets WCAG AA for
  non-text UI (3:1).

## Pitfalls

- **Layout shift**: reserve the bar's height up front; don't let it grow after fonts load.
- **Anchor jumps**: add `scroll-margin-top` equal to nav height on heading targets so the bar doesn't cover section tops.
- **iOS Safari**: rubber-band overscroll can push `ratio` out of range — clamp both ends.
- **Hydration flicker** (SSR frameworks): render `scaleX(0)` server-side and update on mount; avoid measuring width before styles load.
- **Shadow/border on scroll**: toggle an elevation class past a threshold instead of comparing values on every event.

## Variants

- **Section-aware fill**: highlight the nav link of the section currently in
  view (IntersectionObserver) alongside the global progress bar.
- **Read-time label**: pair the bar with "x min left" computed from remaining
  distance and a words-per-minute estimate.
- **Hide-on-down, reveal-on-up**: reclaim space mid-read while keeping the
  indicator available on upward scroll.
