# Tilt Cards Grid

A responsive grid of feature/service cards where each card tilts independently toward the cursor on hover, with a staggered entrance animation triggered on scroll into view.

## Behavior

- Each card rotates in 3D (`rotateX` / `rotateY`) based on cursor position relative to **its own** center — cards never share transform state.
- Entrance: cards start faded and shifted down; an `IntersectionObserver` reveals them one by one with a small per-index delay.
- Touch devices and reduced-motion users skip tilt entirely; entrance degrades to a simple fade-in.

## Implementation Sketch

```html
<section class="tilt-grid" data-tilt-grid>
  <article class="tilt-card">
    <div class="tilt-card__inner"><!-- icon, title, copy --></div>
  </article>
</section>
```

```css
.tilt-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(260px, 1fr));
  gap: 1.5rem;
  perspective: 1200px;
}

.tilt-card {
  opacity: 0;
  transform: translateY(24px);
  transition: opacity 0.5s ease, transform 0.5s ease;
  transition-delay: calc(var(--i, 0) * 80ms);
}

.tilt-card.is-visible {
  opacity: 1;
  transform: translateY(0);
}

.tilt-card__inner {
  transform-style: preserve-3d;
  transition: transform 0.15s ease-out;
}
```

```js
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
const isTouch = matchMedia('(pointer: coarse)').matches;
const io = new IntersectionObserver((entries) => {
  for (const { target } of entries) {
    target.classList.add('is-visible');
    if (!reduceMotion && !isTouch) attachTilt(target);
    io.unobserve(target);
  }
}, { threshold: 0.2 });

document.querySelectorAll('[data-tilt-grid] .tilt-card').forEach((card, i) => {
  card.style.setProperty('--i', Math.min(i, 5)); // cap stagger
  io.observe(card);
});

function attachTilt(card) {
  const inner = card.querySelector('.tilt-card__inner');
  const MAX_DEG = 8;
  let rect = null, raf = null;

  card.addEventListener('pointerenter', () => {
    rect = card.getBoundingClientRect(); // cache once per hover
  });

  card.addEventListener('pointermove', (e) => {
    if (raf || !rect) return;
    raf = requestAnimationFrame(() => {
      const px = ((e.clientX - rect.left) / rect.width - 0.5) * 2;
      const py = ((e.clientY - rect.top) / rect.height - 0.5) * 2;
      const ry = (px * MAX_DEG).toFixed(2);
      const rx = (-py * MAX_DEG).toFixed(2);
      inner.style.transform = `rotateX(${rx}deg) rotateY(${ry}deg)`;
      raf = null;
    });
  });

  card.addEventListener('pointerleave', () => {
    cancelAnimationFrame(raf);
    raf = null;
    inner.style.transform = 'rotateX(0deg) rotateY(0deg)';
  });
}
```

## Usage

1. Wrap the cards in a section with `data-tilt-grid`; give each card class `tilt-card` and put all content inside a `.tilt-card__inner` wrapper.
2. Nothing else to configure — the script assigns `--i`, observes visibility, and attaches tilt only on capable pointers.
3. Keep hover-revealed elements inside `__inner` so they inherit the 3D context.
4. Multiple grids on one page are fine; scope queries per `[data-tilt-grid]` if you want independent stagger ordering.

## Mobile Fallback

- `(pointer: coarse)` or `prefers-reduced-motion` → tilt listeners are never attached; no `pointermove` work happens during scroll.
- Entrance becomes a plain fade-in: same `is-visible` class, transform still applied but subtle enough to read as a fade.
- Stagger delays remain, capped at 5 steps (~400ms max wait) so rows below the fold don't feel sluggish.

## Performance Notes

- Only `transform` and `opacity` are animated — compositor-only, no layout or paint thrash.
- `getBoundingClientRect()` is cached on `pointerenter` and reused for every move event.
- Move handling is throttled through `requestAnimationFrame`, collapsing multiple events per frame into one style write.
- Skip `will-change: transform` on idle cards; apply it only while hovering if profiling demands it.
