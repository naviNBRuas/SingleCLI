# Responsive Layout Strategy for Premium Websites

Premium sites are not desktop pages that shrink. Every breakpoint is a
deliberate re-composition of content, hierarchy, and motion. The goal is that
a visitor on any device feels the layout was designed *for their device*,
never adapted down to it.

## Core principle: substitution over compression

Compression (scaling everything proportionally) is what templates do.
Substitution means swapping structure, media, and interaction patterns at
each breakpoint so each viewport gets its strongest possible composition.

### Concrete example: hero section

**Desktop (>1200px)** — split hero with asymmetric grid:

```css
.hero {
  display: grid;
  grid-template-columns: 7fr 5fr;
  gap: var(--space-10);
  align-items: center;
}
```

Left column: oversized display headline + supporting copy + CTA row.
Right column: full-bleed product imagery or looping video. Both visible
simultaneously; the eye travels headline → image → CTA.

**Tablet (768–1199px)** — stacked but weighted, not centered-everything:

```css
@media (max-width: 1199px) {
  .hero {
    grid-template-columns: 1fr;
    gap: var(--space-6);
  }
  .hero-media {
    order: -1;              /* image leads on touch-first devices */
    max-height: 55vh;
    overflow: hidden;
  }
}
```

Image becomes a cinematic banner above shorter copy. The CTA moves into a
sticky bottom bar if conversion-critical.

**Mobile (<768px)** — single narrative scroll:

- Headline reduced two type steps, line-height tightened.
- Static image replaces video (bandwidth + autoplay policies).
- One primary CTA, full-width, thumb-reachable in the lower third.
- Secondary links collapse into an off-canvas drawer, not a cramped nav bar.

### Concrete example: feature/pricing sections

| Breakpoint | Pattern |
|---|---|
| Desktop | 3–4 column card row, hover elevation reveals detail |
| Tablet | 2-column grid; third card spans both columns |
| Mobile | Horizontal snap-scroll carousel with peek of next card |

The tablet rule "third card spans" is a substitution decision: a lone card
in row three looks broken, a spanning card looks intentional.

## Asymmetric layouts with CSS Grid

Symmetry reads generic. Premium compositions use tension: unequal columns,
overlapping layers, deliberate whitespace imbalance.

### The 12-column editorial overlap

```css
.editorial {
  display: grid;
  grid-template-columns: repeat(12, 1fr);
}
.editorial-text {
  grid-column: 2 / 8;
  grid-row: 1;
  z-index: 2;
}
.editorial-image {
  grid-column: 6 / 13;
  grid-row: 1;
  margin-top: calc(var(--space-12) * -0.4); /* vertical offset = depth */
}
```

Text overlaps the image by two columns; the negative top offset staggers
them vertically. Result: layered, magazine-like depth instead of side-by-side
blocks. On mobile, collapse to sequential stacking with the image clipped:

```css
@media (max-width: 767px) {
  .editorial-text { grid-column: 1 / -1; }
  .editorial-image {
    grid-column: 3 / -1;   /* bleed off the right edge */
    margin-top: calc(var(--space-6) * -1);
  }
}
```

Keeping the edge bleed preserves asymmetry even in a single column.

### Flexbox for controlled asymmetry

Grid owns page-level composition; Flexbox excels at component internals
where items have intrinsic sizes:

```css
.stat-row {
  display: flex;
  gap: var(--space-6);
  align-items: baseline;
}
.stat-row .stat-value { font-size: clamp(3rem, 6vw, 5rem); }
.stat-row .stat-label { max-width: 16ch; }   /* forces ragged wrap */
```

`align-items: baseline` aligns numerals of differing sizes along one optical
line — a small typographic detail that separates crafted UIs from defaults.

## Container queries: component-level responsiveness

Media queries answer "how wide is the viewport?" Container queries answer
"how wide is my parent?" This decouples components from page layout — the
same card behaves correctly in a hero slot and a sidebar without duplicate
CSS.

```css
.card-scope { container-type: inline-size; }

@container (min-width: 480px) {
  .card {
    display: grid;
    grid-template-columns: 1fr 2fr;  /* media left, copy right */
  }
}

@container (max-width: 479px) {
  .card {
    display: block;                  /* media stacks above copy */
  }
}
```

Practical rules:

- Set `container-type: inline-size` on wrappers, never on the styled
  component itself (creates a circular size dependency).
- Prefer container queries inside reusable components; keep media queries
  for page-level structure like grids and section ordering.
- Combine both: media query reorders sections, container query adapts the
  cards living inside them.

## Fluid typography with clamp()

Fixed breakpoints force discrete type jumps; `clamp()` interpolates
continuously between them:

```css
h1 { font-size: clamp(2.25rem, 1rem + 4vw, 5rem); }
```

Why it matters for premium work:

1. **No jarring jumps.** Resizing the window morphs scale smoothly — the
   page feels alive rather than snapping between presets.
2. **Fewer breakpoints to maintain.** Typography self-adjusts; media
   queries are freed up for structural substitutions only.
3. **Accessibility preserved.** Unlike pure `vw` sizing, `clamp()` floors
   and ceilings keep text within readable bounds; pair with rem-based min
   values so user font preferences still scale the floor.
4. **Optical balance.** Display headlines stay proportional to whitespace
   and imagery across devices, protecting the designed rhythm.

Apply the same technique to spacing (`clamp()` on section padding) so
vertical rhythm breathes on large screens and tightens gracefully on small.

## Checklist before shipping a breakpoint set

- [ ] Each breakpoint has at least one intentional substitution, not just
      resized columns.
- [ ] No orphaned or awkwardly spanning grid items at any width.
- [ ] Components adapt via container queries where they can appear in
      multiple slots.
- [ ] Type and spacing use `clamp()` ranges verified at 320px, 768px,
      1440px, and ultrawide (2560px).
- [ ] Touch targets ≥44px below 1024px; sticky CTAs tested against iOS
      safe areas.
