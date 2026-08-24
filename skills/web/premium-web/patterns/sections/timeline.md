# Timeline (scroll-driven vertical)

A vertical timeline where a progress line fills as the user scrolls and milestone
nodes animate into view. Uses `IntersectionObserver` + CSS custom properties —
no scroll-jacking, no layout thrash, readable without JavaScript.

## Structure

```html
<section class="timeline">
  <h2 id="timeline-heading">Company history</h2>

  <ol class="timeline__list" aria-labelledby="timeline-heading">
    <li class="timeline__item">
      <span class="timeline__marker" aria-hidden="true"></span>
      <article class="timeline__card">
        <time datetime="2019-03">March 2019</time>
        <h3>Founded</h3>
        <p>Two people, one garage, zero customers.</p>
      </article>
    </li>
    <!-- repeat <li> per milestone -->
  </ol>
</section>
```

- Root is an `<ol>`: milestones are ordered content, so use real list semantics.
- One `<li>` per milestone. Never swap these for `<div role="list">` — native beats ARIA.
- `.timeline__marker` (the dot) is decorative: `aria-hidden="true"`, or better, a `::before` pseudo-element on the `<li>`.
- `<time datetime>` gives machine-readable dates.

## Implementation sketch

Track + progress fill are pseudo-elements on the list; JS drives one custom property:

```css
.timeline__list {
  position: relative;
  padding-inline-start: 2.5rem;
}

/* Track */
.timeline__list::before,
.timeline__list::after {
  content: "";
  position: absolute;
  inset-block: 0;
  inset-inline-start: 0.5rem;
  width: 2px;
}

.timeline__list::before { background: var(--line, #e2e2e2); }

/* Progress fill — scaleY driven by --progress (0..1), origin top */
.timeline__list::after {
  background: var(--accent, #6366f1);
  transform-origin: top;
  transform: scaleY(var(--progress, 0));
}
```

```js
const list = document.querySelector(".timeline__list");

// 1. Progress line: map list position in viewport to 0..1.
function updateProgress() {
  const r = list.getBoundingClientRect();
  const anchor = innerHeight * 0.6; // fill toward a point 60% down the viewport
  const p = Math.min(1, Math.max(0, (anchor - r.top) / r.height));
  list.style.setProperty("--progress", p);
}
addEventListener("scroll", () => requestAnimationFrame(updateProgress), { passive: true });
updateProgress();

// 2. Node reveal: animate each item once when it enters the viewport.
if (!matchMedia("(prefers-reduced-motion: reduce)").matches) {
  const io = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (e.isIntersecting) {
        e.target.classList.add("is-visible");
        io.unobserve(e.target);
      }
    }
  }, { threshold: 0.3 });
  list.querySelectorAll(".timeline__item").forEach((el) => io.observe(el));
} else {
  document.documentElement.classList.add("no-motion");
}
```

Reveal styles — gated behind a `.js` class so content stays visible without JS:

```css
.js .timeline__item {
  opacity: 0;
  translate: 0 1.5rem;
  transition: opacity 0.5s ease, translate 0.5s ease;
}
.js .timeline__item.is-visible { opacity: 1; translate: 0 0; }
.no-motion .timeline__item { opacity: 1; translate: none; transition: none; }
```

## Usage

1. Copy the markup; keep the `<ol>`/`<li>` structure intact.
2. Set `--accent` (and optionally `--line`) on `.timeline` to match your theme.
3. Add the `js` class to `<html>` early (one-liner inline script) for progressive enhancement.
4. Scope the JS by container ID if multiple timelines share a page.
5. Keep items reasonably spaced; the progress math maps the whole list box, not per item.

## Mobile behavior

Below 640px, collapse to a simplified single column:

- Track hugs the left edge; markers shrink to 10px dots.
- Cards take full width below/right of the marker — drop any desktop alternating
  left/right layout entirely; it reads poorly at narrow widths.
- Shorter reveal offset (`translate: 0 1rem`) to avoid clipping at the viewport edge.

```css
@media (max-width: 640px) {
  .timeline__list { padding-inline-start: 1.5rem; }
  .timeline__marker { inline-size: 10px; block-size: 10px; }
  .js .timeline__item { translate: 0 1rem; }
}
```

## Accessibility

- Semantics come free from `<ol>`, `<li>`, `<h3>`, `<time>` — screen readers announce
  "list, N items" and users navigate milestone by milestone. This is why the pattern
  must not be rebuilt from stacked divs.
- Markers and both line pseudo-elements are decorative; nothing meaningful lives outside the text.
- `prefers-reduced-motion` disables the reveal transition and renders everything static.
- Without JS (no `.js` class), all items are visible and the progress line sits empty — content unaffected.
