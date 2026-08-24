# Animated CTA Button

Premium call-to-action pattern for marketing pages and hero sections. Three coordinated micro-interactions in one reusable component:

1. **Fill-sweep** — a solid layer sweeps across the button on hover, flipping text color.
2. **Arrow-slide** — stacked arrow icons swap places so the arrow appears to reload from the left edge.
3. **Press scale** — subtle `scale(0.97)` on active press for tactile depth.

Pure CSS, no JS required.

## Implementation sketch

```html
<a class="cta" href="/signup">
  <span class="cta__label">Start free trial</span>
  <span class="cta__arrow" aria-hidden="true">
    <svg viewBox="0 0 16 16"><path d="M2 8h10M8 3l5 5-5 5"/></svg>
    <svg viewBox="0 0 16 16"><path d="M2 8h10M8 3l5 5-5 5"/></svg>
  </span>
</a>
```

```css
.cta {
  --cta-bg: #111;
  --cta-fg: #fff;
  --cta-accent: #4f46e5;
  position: relative;
  display: inline-flex;
  align-items: center;
  gap: 0.75rem;
  padding: 0.9rem 1.6rem;
  border-radius: 999px;
  background: var(--cta-bg);
  color: var(--cta-fg);
  font-weight: 600;
  text-decoration: none;
  overflow: hidden; /* clips the sweep layer */
  transition: transform 150ms cubic-bezier(0.2, 0, 0, 1);
}
.cta::before {
  content: "";
  position: absolute;
  inset: 0;
  background: var(--cta-accent);
  transform: translateX(-101%);
  transition: transform 300ms cubic-bezier(0.65, 0, 0.35, 1);
}
.cta:hover::before { transform: translateX(0); }

.cta__label, .cta__arrow { position: relative; z-index: 1; transition: color 300ms ease; }
.cta:hover .cta__label,
.cta:hover .cta__arrow { color: var(--cta-fg); }

/* Arrow-slide: two stacked arrows inside .cta__arrow */
.cta__arrow { display: inline-grid; }
.cta__arrow svg {
  grid-area: 1 / 1;
  width: 1rem; height: 1rem;
  stroke: currentColor; fill: none; stroke-width: 1.75;
  transition: transform 250ms cubic-bezier(0.65, 0, 0.35, 1);
}
.cta__arrow svg:nth-child(2) { transform: translateX(-150%); opacity: 0; }
.cta:hover .cta__arrow svg:nth-child(1) { transform: translateX(120%); opacity: 0; }
.cta:hover .cta__arrow svg:nth-child(2) { transform: translateX(0); opacity: 1; }
.cta:active { transform: scale(0.97); }
```

## Usage

- Reserve for **one primary action per view**; secondary actions stay static.
- Sweep duration 200–350ms — slower reads sluggish, faster reads glitchy.
- Use `cubic-bezier(0.65, 0, 0.35, 1)` (easeInOutCubic) so enter/exit feel symmetric.
- Drop the markup + CSS into any framework component; all state is pseudo-class driven.
- GSAP optional: only if already bundled, drive press scale from `pointerdown` via a paused `gsap.timeline()`; otherwise pseudo-classes cover everything.

## Touch devices

Hover is sticky or absent on touch. Gate every `:hover` rule behind pointer capability:

```css
@media (hover: hover) and (pointer: fine) {
  /* all :hover rules live here */
}
```

On touch, `:active` alone provides tap feedback: the press-scale fires on tap-down, before navigation. Add a JS radial pulse only if the button sits on dark imagery where the scale is hard to perceive.

## Accessibility

- Focus visible and **distinct from hover** — an outline ring, never the sweep:

```css
.cta:focus-visible {
  outline: 2px solid var(--cta-accent);
  outline-offset: 3px;
}
```

- Never remove `outline` without a `:focus-visible` replacement.
- Arrows are decorative (`aria-hidden`); the label carries full meaning.
- Respect reduced motion:

```css
@media (prefers-reduced-motion: reduce) {
  .cta, .cta::before, .cta__arrow svg { transition-duration: 0ms; }
}
```

- Contrast must hold in both resting and swept states (WCAG AA minimum).
