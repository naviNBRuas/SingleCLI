# Text Reveal (Scroll-Triggered Split-Text)

Progressively reveals words or characters as they enter the viewport.
Common on hero headlines and section intros in premium marketing sites.

## When to use

- Hero titles, section headings, short paragraphs where emphasis matters.
- Do **not** apply to long body copy: splitting text hurts accessibility
  and reading flow at length.

## Implementation sketch (vanilla, IntersectionObserver)

```js
const prefersReduced = window.matchMedia(
  '(prefers-reduced-motion: reduce)'
).matches;

function splitWords(el) {
  const words = el.textContent.trim().split(/\s+/);
  el.textContent = '';
  words.forEach((word, i) => {
    const outer = document.createElement('span');
    outer.className = 'reveal-word';
    const inner = document.createElement('span');
    inner.className = 'reveal-inner';
    inner.textContent = word;
    inner.style.transitionDelay = `${i * 40}ms`;
    outer.appendChild(inner);
    el.appendChild(outer);
    if (i < words.length - 1) el.appendChild(document.createTextNode(' '));
  });
}

function initTextReveal(selector = '[data-text-reveal]') {
  document.querySelectorAll(selector).forEach((el) => {
    if (prefersReduced) return; // leave static
    splitWords(el);
    const io = new IntersectionObserver(
      ([entry]) => {
        if (!entry.isIntersecting) return;
        el.classList.add('is-revealed');
        io.disconnect(); // reveal once
      },
      { threshold: 0.35 }
    );
    io.observe(el);
  });
}
```

```css
.reveal-word {
  display: inline-block;
  overflow: hidden;
  vertical-align: top;
}
.reveal-inner {
  display: inline-block;
  transform: translateY(110%);
  opacity: 0;
  transition:
    transform 0.6s cubic-bezier(0.22, 1, 0.36, 1),
    opacity 0.6s ease;
  will-change: transform, opacity;
}
.is-revealed .reveal-inner {
  transform: translateY(0);
  opacity: 1;
}
```

## Usage

1. Add `data-text-reveal` to the heading/paragraph element.
2. Call `initTextReveal()` after DOM ready (`DOMContentLoaded`).
3. Tune stagger via `transitionDelay` step (30–50ms reads well).
4. For per-character reveals, swap `split(/\s+/)` for `[...text]`
   and reduce the stagger to ~15–25ms.

### GSAP ScrollTrigger variant

If GSAP is already loaded, prefer `ScrollTrigger` + `SplitText`:

```js
gsap.registerPlugin(ScrollTrigger);
const split = new SplitText('[data-text-reveal]', { type: 'words' });
gsap.from(split.words, {
  yPercent: 110,
  autoAlpha: 0,
  stagger: 0.04,
  ease: 'power3.out',
  scrollTrigger: { trigger: '[data-text-reveal]', start: 'top 75%', once: true },
});
```

## Reduced motion

`prefers-reduced-motion: reduce` skips splitting entirely — text renders
normally with zero animation. The CSS-only equivalent:

```css
@media (prefers-reduced-motion: reduce) {
  .reveal-inner { transform: none; opacity: 1; transition: none; }
}
```

## Responsive behavior

- Word-level splits reflow naturally because each word stays inline-block;
  line breaks land between words exactly like plain text.
- Character-level reveals break word wrapping — only use them for single
  lines (hero titles) or add explicit `<br>` control at breakpoints.
- Recalculate nothing on resize: transforms don't invalidate layout, so
  the reveal survives orientation changes untouched.

## Performance notes

- Animate only `transform` and `opacity` — both are GPU-composited and
  never trigger layout or paint. Never animate `top`, `height`, or `margin`.
- `overflow: hidden` on the wrapper creates the mask without filters,
  which keeps each word on its own cheap layer.
- Disconnect the `IntersectionObserver` after firing; one-shot reveals
  shouldn't keep observing during scroll.
- Batch all DOM reads/writes inside `initTextReveal()` before scrolling
  starts; do not measure elements inside the observer callback (that
  forces synchronous layout mid-scroll → jank).
- Keep `will-change` scoped to `.reveal-inner` and remove it (or let the
  element settle) after the transition ends to avoid layer explosion on
  long pages.
