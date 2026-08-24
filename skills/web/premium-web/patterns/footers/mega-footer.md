# Mega Footer

A premium closing block for marketing sites and product landing pages: a
multi-column link grid, a large closing wordmark/CTA treatment, and a subtle
background texture. The footer is the last thing a visitor sees — treat it as
a conversion surface, not an afterthought.

## When to use

- Marketing homepages and landing pages with 3+ link groups.
- Product sites that want a strong brand "signature" at the end of the page.
- Sites that need deep-link discovery without cluttering the header.

Do not use for single-purpose apps or docs sites — use a compact utility
footer there instead.

## Structure

```html
<footer class="mega-footer">
  <div class="mega-footer__inner">
    <div class="mega-footer__cta">
      <p class="mega-footer__tagline">Build something worth signing.</p>
      <a class="mega-footer__btn" href="/signup">Start free</a>
    </div>

    <nav class="mega-footer__grid" aria-label="Footer">
      <section class="mega-footer__col" aria-labelledby="f-product">
        <h2 id="f-product" class="mega-footer__heading">Product</h2>
        <ul>
          <li><a href="/features">Features</a></li>
          <li><a href="/pricing">Pricing</a></li>
        </ul>
      </section>
      <section class="mega-footer__col" aria-labelledby="f-company">
        <h2 id="f-company" class="mega-footer__heading">Company</h2>
        <ul>
          <li><a href="/about">About</a></li>
          <li><a href="/careers">Careers</a></li>
        </ul>
      </section>
      <!-- repeat columns as needed -->
    </nav>

    <div class="mega-footer__legal">
      <span class="mega-footer__wordmark">ACME</span>
      <p>&copy; 2026 Acme Inc. All rights reserved.</p>
    </div>
  </div>
</footer>
```

## Implementation sketch

```css
.mega-footer {
  position: relative;
  padding-block: clamp(4rem, 10vw, 8rem) 2.5rem;
  background-color: var(--footer-bg, #0c0d10);
  color: #e7e8ea;
}

/* Subtle texture: layered radial gradients + faint noise via SVG data URI */
.mega-footer::before {
  content: "";
  position: absolute;
  inset: 0;
  pointer-events: none;
  opacity: 0.35;
  background-image:
    radial-gradient(60% 80% at 80% 0%, rgba(255, 255, 255, 0.06), transparent 70%),
    url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='120' height='120'%3E%3Cfilter id='n'%3E%3CfeTurbulence type='fractalNoise' baseFrequency='0.9' numOctaves='2'/%3E%3CfeColorMatrix values='0 0 0 0 1 0 0 0 0 1 0 0 0 0 1 0 0 0 0.04 0'/%3E%3C/filter%3E%3Crect width='120' height='120' filter='url(%23n)'/%3E%3C/svg%3E");
}

.mega-footer__inner {
  max-width: 72rem;
  margin-inline: auto;
  display: grid;
  gap: 3rem;
  grid-template-columns: minmax(16rem, 1fr) minmax(0, 2fr);
  grid-template-areas: "cta grid" "legal legal";
}

.mega-footer__wordmark {
  font-size: clamp(3rem, 12vw, 9rem);
  font-weight: 800;
  letter-spacing: -0.04em;
  line-height: 0.9;
  background: linear-gradient(180deg, #fff 30%, rgba(255, 255, 255, 0.25));
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}
```

## Responsive behavior

Breakpoints are indicative; tune to your design tokens.

| Viewport | Layout |
|---|---|
| `≥1024px` | CTA panel left, 3–4 column link grid right; giant wordmark spans the full width below. |
| `640–1023px` | CTA above the grid; grid becomes 2 columns; wordmark scales down via `clamp()`. |
| `<640px` | Each column collapses into a `<details>`-style **accordion**; only headings visible until tapped. |

Mobile accordion sketch:

```html
<section class="mega-footer__col" aria-labelledby="f-product-m">
  <details open>
    <summary id="f-product-m">Product</summary>
    <ul><!-- links --></ul>
  </details>
</section>
```

```css
@media (max-width: 639px) {
  .mega-footer .mega-footer__col details summary { cursor: pointer; }
  .mega-footer .mega-footer__col ul { margin-top: 0.75rem; }
}
```

Use JS-free `<details>/<summary>` when possible; if you need animated height,
progressively enhance — keep content reachable without JavaScript.

## Accessibility

- Landmarks: exactly one `<footer>` per page region; wrap the link groups in
  `<nav aria-label="Footer">` so screen reader users can jump to them.
- Give each column an accessible heading (`<h2>` or a heading level consistent
  with your outline). Do not fake headings with bolded `<span>`s.
- Contrast: footer text on dark backgrounds must meet WCAG AA (4.5:1 body,
  3:1 large text). Muted link colors commonly fail — check `#8b8d91` on
  `#0c0d10`.
- Focus states: keep visible `:focus-visible` rings on all links/buttons;
  never remove outlines inside the footer.
- Texture must be decorative only (`pointer-events: none`, no meaningful
  content), and gradients behind text must not reduce contrast.
- Accordion pattern on mobile: `<summary>` is natively keyboard-operable;
  avoid custom ARIA accordions unless you implement `aria-expanded` correctly.

## Tokens

| Token | Default | Purpose |
|---|---|---|
| `--footer-bg` | `#0c0d10` | Base background |
| `--footer-fg` | `#e7e8ea` | Body/link color |
| `--footer-muted` | `#9a9ca1` | Headings, legal text |
| `--texture-opacity` | `0.35` | Noise/gradient layer |

## Checklist

- [ ] Single `<footer>` landmark, one labelled `<nav>` for the grid.
- [ ] Wordmark scales with `clamp()`; no horizontal overflow at 320px.
- [ ] Columns become accordions under 640px without JS dependency.
- [ ] All links resolve to real routes; no dead `#` placeholders ship.
