# Spotlight CTA

A call-to-action card that renders a soft radial glow which follows the
visitor's cursor across its surface. The effect is driven entirely by two CSS
custom properties (`--x`, `--y`) updated from a single pointer listener — no
per-frame layout thrash, no canvas, no library.

## When to use

- Premium landing pages where the primary CTA should feel tactile.
- Dark-themed hero sections; the glow reads as light hitting a surface.
- Cards or panels where you want affordance without extra chrome.

Avoid on low-contrast light themes (the glow disappears) and touch-only
contexts without a fallback (see Accessibility).

## Markup

```html
<a class="spotlight-cta" href="/pricing">
  <span class="spotlight-cta__label">Start free</span>
  <span class="spotlight-cta__glow" aria-hidden="true"></span>
</a>
```

Keep the glow as a dedicated child layer so it can be composited
independently and never affects text rendering.

## Styles

```css
.spotlight-cta {
  --x: 50%;
  --y: 50%;
  --spot-color: 120 160 255;
  --spot-size: 180px;

  position: relative;
  overflow: hidden;
  isolation: isolate;
  border-radius: 14px;
}

.spotlight-cta__glow {
  position: absolute;
  inset: 0;
  z-index: -1;
  opacity: 0;
  transition: opacity 240ms ease;

  background: radial-gradient(
    var(--spot-size) circle at var(--x) var(--y),
    rgb(var(--spot-color) / 0.35),
    transparent 70%
  );
}

.spotlight-cta:hover .spotlight-cta__glow {
  opacity: 1;
}
```

Key details:

- `overflow: hidden` clips the gradient to the card bounds.
- `isolation: isolate` keeps `z-index: -1` inside the card.
- Opacity transitions in/out so the glow fades instead of popping.

## Behavior

```js
const cta = document.querySelector(".spotlight-cta");

cta.addEventListener("pointermove", ({ clientX, clientY }) => {
  const rect = cta.getBoundingClientRect();
  cta.style.setProperty("--x", `${clientX - rect.left}px`);
  cta.style.setProperty("--y", `${clientY - rect.top}px`);
});
```

- Custom properties are inherited by descendants automatically, so setting
  them once on the container updates the gradient layer.
- Setting properties does not trigger style recalculation of geometry;
  the paint is cheap and GPU-friendly.
- Use `pointermove`, not `mousemove`, so pen and touch pointers work too.

## Performance notes

- Batch reads/writes: read `getBoundingClientRect()` per event is fine here,
  but cache it on `pointerenter`/resize if profiling shows pressure.
- Prefer `rgb(... / alpha)` over box-shadow blur for the glow — gradients
  repaint faster than large blurred shadows.
- Never animate `background-position`; animating the custom properties'
  consumers (paint) is already optimal.

## Accessibility

- The glow is decorative: keep it `aria-hidden`.
- Focus must remain visible independent of hover — pair the glow with an
  explicit `:focus-visible` outline on the CTA.
- Wrap pointer logic in `(matchMedia("(hover: hover)").matches)` so touch
  devices skip it; the button still works, just without the glow.
- Respect `prefers-reduced-motion`: the effect is positional, not animated,
  but disable the opacity transition under reduced motion for consistency.
