# Minimal Footer

A single-row footer for premium sites: wordmark, a handful of links, copyright,
and social icons sharing one baseline. For sites whose pages end cleanly and
don't need a mega-footer's link farm or newsletter block.

## When to use

- Marketing sites, portfolios, docs shells, and product pages with fewer than six footer destinations.
- Pages where the primary CTA above the fold matters more than footer real estate.
- Layouts where a tall footer would visually outweigh the page content.

Skip it when legal or compliance requirements force 10+ links, or users expect
a sitemap-style directory — reach for the mega-footer pattern instead.

## Implementation sketch

```html
<footer class="footer">
  <div class="footer__inner">
    <a href="/" class="footer__brand" aria-label="Acme — home">
      <svg aria-hidden="true"><!-- logo mark --></svg>
      <span>Acme</span>
    </a>

    <nav class="footer__nav" aria-label="Footer">
      <a href="/pricing">Pricing</a>
      <a href="/docs">Docs</a>
      <a href="/blog">Blog</a>
      <a href="/contact">Contact</a>
    </nav>

    <div class="footer__meta">
      <ul class="footer__social">
        <li>
          <a href="https://x.com/acme" aria-label="Acme on X">
            <svg aria-hidden="true"><!-- icon --></svg>
          </a>
        </li>
      </ul>
      <small class="footer__copy">&copy; 2026 Acme, Inc.</small>
    </div>
  </div>
</footer>
```

```css
.footer__inner {
  display: flex;
  align-items: center;
  gap: var(--space-5);
  padding-block: var(--space-6);
  border-top: 1px solid var(--border-subtle);
}
.footer__nav { display: flex; gap: var(--space-4); margin-inline-start: auto; }
.footer__social { display: flex; gap: var(--space-3); }
.footer__social svg { width: 1.125rem; height: 1.125rem; }
```

## Usage

1. Keep it to one row: brand left, nav right of it, social + copyright far right.
2. Cap nav at four or five links; anything more belongs in a mega-footer.
3. Reuse the same border and surface tokens as your header so the top and bottom frames match.
4. Generate the copyright year at build time rather than hardcoding it.
5. Let social icons inherit `currentColor` so hover states stay consistent.

## Responsive behavior

- 768px and up: everything sits on one row as sketched.
- Below 768px: stack into two rows — brand plus copyright on top, nav plus social below, both centered; tighten gaps to `--space-3`.
- Never truncate or hide links at small sizes; reflow instead.

```css
@media (max-width: 767px) {
  .footer__inner { flex-direction: column; text-align: center; }
  .footer__nav { margin-inline-start: 0; flex-wrap: wrap; justify-content: center; }
}
```

## Accessibility

- Wrap the links in `<nav aria-label="Footer">` so screen readers announce the region distinctly from the main nav.
- Icon-only social links need an explicit `aria-label`; never rely on tooltips or `title`.
- Mark decorative logos and glyphs `aria-hidden="true"` and put the label on the wrapping anchor.
- Text must meet WCAG AA contrast (4.5:1, 3:1 for large text and icons), including on tinted or dark footers.
- Keep visible focus rings on every footer link; do not suppress outlines.
- Ensure tap targets are at least 44x44px using padding, not larger glyphs alone.

## Related

- `patterns/headers/minimal-header.md` — the matching top frame.
