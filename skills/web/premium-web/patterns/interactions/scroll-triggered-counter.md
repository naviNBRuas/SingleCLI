---
title: Scroll-Triggered Counter
type: interaction
tags: [animation, intersection-observer, stats]
---

# Scroll-Triggered Counter

Animates a numeric stat counting up from zero the first time it scrolls into
view — "10,000+ users", "99.9% uptime". Fires once per element per page load;
the real number ships in the HTML, so no-JS visitors, crawlers, and
reduced-motion users always see the final value immediately.

## Markup

```html
<p class="stat">
  <span class="sr-only" aria-live="polite"></span>
  <span data-counter="10000" data-suffix="+" data-label="users">10,000+ users</span>
</p>
```

- `data-counter`: digits only, no separators.
- `data-suffix` / `data-label`: non-numeric decoration around the number.
- `.sr-only` is your standard visually-hidden utility class.

## Script

```js
const reducedMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;

function tween(el, done) {
  const target   = Number(el.dataset.counter);
  const rest     = el.dataset.suffix || "";
  const label    = el.dataset.label ? ` ${el.dataset.label}` : "";
  const duration = 1200;
  const ease     = t => 1 - Math.pow(1 - t, 3); // ease-out cubic
  let start;

  const frame = now => {
    start ??= now;
    const p = Math.min((now - start) / duration, 1);
    const n = Math.round(target * ease(p)).toLocaleString();
    el.textContent = n + rest + label;
    p < 1 ? requestAnimationFrame(frame) : done();
  };
  requestAnimationFrame(frame);
}

if (!reducedMotion) {
  const observer = new IntersectionObserver((entries, obs) => {
    entries.forEach(({ isIntersecting, target }) => {
      if (!isIntersecting) return;
      obs.unobserve(target);                      // fire once
      target.setAttribute("aria-hidden", "true"); // mute intermediate ticks
      tween(target, () => {
        const live = target.closest(".stat")?.querySelector(".sr-only");
        if (live) live.textContent = target.textContent; // announce once
      });
    });
  }, { threshold: 0.4 });

  document.querySelectorAll("[data-counter]").forEach(el => observer.observe(el));
}
```

## Usage

- Include the script once near the end of `<body>`; it wires up every
  `[data-counter]` on the page through a single shared observer.
- Tune the feel with `duration` (ms) and `threshold` (how much of the element
  must be visible before triggering; 0.3–0.5 suits stat bands).
- `toLocaleString()` formats per the visitor's locale; pass an explicit
  locale if the design demands fixed separators.
- No dependencies, no build step.

## Reduced motion

When `prefers-reduced-motion: reduce` matches, the script exits before the
observer is ever created, so `textContent` is never rewritten and the static
final value stays in place. There is no layout shift either way, because the
initial paint already contains the complete string.

## Accessibility

- The ticking span becomes `aria-hidden="true"` only while animating;
  screen readers never perceive the intermediate values.
- The `aria-live="polite"` region is populated exactly once, on completion,
  so assistive tech announces "10,000+ users" a single time.
- Never attach `aria-live` to the animated element itself — most screen
  readers would attempt to announce nearly every frame.
- With JavaScript disabled the pattern degrades to plain static text.
