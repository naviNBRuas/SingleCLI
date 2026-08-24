# Glass Navigation

A fixed navigation bar that is fully transparent at the top of the page and
transitions into a frosted-glass surface once the user scrolls. Uses
`backdrop-filter: blur()` over a semi-transparent background, reinforced with a
layered hairline border and stacked shadows for depth.

## When to use

- Marketing sites, portfolios, or product pages with hero sections where the
  nav should visually merge with content at rest.
- Pages with long scroll where a persistent, low-noise header improves wayfinding.
- Dark and light themes alike — the recipe below adapts via CSS custom properties.

Avoid it when the page has dense, high-frequency content scrolling under the
header: constant blur re-compositing can feel busy and costs GPU time.

## Markup

```html
<header class="glass-nav" data-state="top">
  <a class="glass-nav__brand" href="/">Brand</a>
  <nav class="glass-nav__links" aria-label="Primary">
    <a href="/features">Features</a>
    <a href="/pricing">Pricing</a>
    <a href="/docs">Docs</a>
  </nav>
</header>
```

## CSS

```css
.glass-nav {
  --nav-bg: 255 255 255;
  --nav-fg: #1a1d21;
  --nav-border: rgb(var(--nav-bg) / 0.55);

  position: fixed;
  inset-inline: 0;
  top: 0;
  z-index: 100;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 1rem 2rem;
  color: var(--nav-fg);
  background-color: transparent;
  border-bottom: 1px solid transparent;
  transition:
    background-color 240ms ease,
    border-color 240ms ease,
    box-shadow 240ms ease,
    padding 240ms ease;
}

/* Scrolled state: the glass layer */
.glass-nav[data-state="scrolled"] {
  padding-block: 0.625rem;
  background-color: rgb(var(--nav-bg) / 0.62);
  -webkit-backdrop-filter: blur(14px) saturate(1.5);
  backdrop-filter: blur(14px) saturate(1.5);

  /* Layered edge: inner highlight + hairline + soft drop shadow */
  border-bottom-color: var(--nav-border);
  box-shadow:
    inset 0 1px 0 rgb(255 255 255 / 0.35),
    0 1px 0 rgb(var(--nav-bg) / 0.25),
    0 8px 24px rgb(0 0 0 / 0.12);
}

@media (prefers-reduced-motion: reduce) {
  .glass-nav { transition-duration: 1ms; }
}
```

For dark themes swap the tokens:

```css
[data-theme="dark"] .glass-nav {
  --nav-bg: 18 20 24;
  --nav-fg: #f4f6f8;
}
```

## Scroll-state behavior

Toggle `data-state` from JS using a scroll listener guarded by
`requestAnimationFrame`. The threshold matches one viewport-height fraction so
the flip happens after the hero has visibly moved.

```js
const nav = document.querySelector(".glass-nav");
let ticking = false;

function update() {
  nav.dataset.state = window.scrollY > 24 ? "scrolled" : "top";
  ticking = false;
}

window.addEventListener("scroll", () => {
  if (!ticking) {
    ticking = true;
    requestAnimationFrame(update);
  }
}, { passive: true });

update();
```

Keep the listener `{ passive: true }`; the handler never calls
`preventDefault`, so blocking scroll waiting on it would only add jank.

## Fallback without backdrop-filter

Browsers without `backdrop-filter` (or with it disabled) would otherwise show
text floating over unreadable page content behind a 62% tint alone. Detect
support and fall back to an opaque bar:

```css
@supports not ((backdrop-filter: blur(1px)) or (-webkit-backdrop-filter: blur(1px))) {
  .glass-nav[data-state="scrolled"] {
    background-color: rgb(var(--nav-bg) / 0.98);
    backdrop-filter: none; /* explicit: never half-applied */
  }
}
```

The opaque fallback keeps the same transition timing, so users on either path
see identical motion.

## Accessibility

- **Contrast:** the glass sits over arbitrary content, so text contrast varies.
  Keep body text at `#1a1d21`-equivalent luminance on light glass (≥ 7:1) and
  verify the worst case by screenshotting the nav over your busiest section.
  If contrast dips below 4.5:1 anywhere, raise `--nav-bg` opacity toward 0.85.
- **Focus states:** links need visible focus that survives the blur layer:

```css
.glass-nav a:focus-visible {
  outline: 2px solid currentColor;
  outline-offset: 3px;
  border-radius: 4px;
}
```

- **Motion:** the state transition respects `prefers-reduced-motion` above.
- **Semantics:** keep the bar as a real `<header>`/`<nav>` pair; do not build
  it from divs. Screen readers announce landmarks, not visual effects.
- **Hit targets:** at the compact scrolled padding (`0.625rem`), confirm links
  still meet the 44×44px minimum touch target via extra inline padding.

## Tuning

| Knob | Effect |
|---|---|
| `blur(14px)` | Higher = frostier, more GPU cost |
| `saturate(1.5)` | Boosts colors bleeding through; drop to 1 for neutral glass |
| `rgb(... / 0.62)` | Opacity trade-off: readability vs. transparency |
| `inset 0 1px 0 ...` | Top highlight line; remove for flat design |
