# Logo Wall

A responsive grid of client/partner logos that renders in grayscale by
default and transitions to full color on hover. Optional staggered fade-in
on scroll for a premium reveal moment.

## When to use

- Social proof sections on landing pages ("Trusted by teams at ...")
- Partner/integration directories where many logos must coexist calmly
- Press or "featured in" strips

Avoid when logos carry legal/branding requirements that forbid color
treatment changes, or when there are fewer than 4 logos (use inline badges).

## Implementation sketch

```html
<section class="logo-wall" aria-label="Companies using our product">
  <h2 class="logo-wall__title">Trusted by teams at</h2>
  <ul class="logo-wall__grid">
    <li class="logo-wall__item">
      <img src="/logos/acme.svg" alt="Acme Corp" loading="lazy" />
    </li>
    <li class="logo-wall__item">
      <img src="/logos/globex.svg" alt="Globex" loading="lazy" />
    </li>
    <!-- repeat per client -->
  </ul>
</section>
```

```css
.logo-wall__grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: clamp(1.5rem, 4vw, 3rem);
  align-items: center;
  justify-items: center;
}

.logo-wall__item img {
  height: 40px;
  width: auto;
  filter: grayscale(100%);
  opacity: 0.75;
  transition: filter 250ms ease, opacity 250ms ease, transform 250ms ease;
}

.logo-wall__item img:hover,
.logo-wall__item img:focus-visible {
  filter: grayscale(0%);
  opacity: 1;
  transform: translateY(-2px);
}

@media (prefers-reduced-motion: reduce) {
  .logo-wall__item img {
    transition: none;
    transform: none;
  }
}
```

### Scroll fade-in (optional)

Add `.is-visible` via an IntersectionObserver; each item gets
`transition-delay: calc(var(--i) * 60ms)` with `--i` set inline per item.

```css
.logo-wall__item {
  opacity: 0;
}
.logo-wall__item.is-visible {
  animation: logo-fade 500ms ease forwards;
}
@keyframes logo-fade {
  from { opacity: 0; transform: translateY(8px); }
  to   { opacity: 1; transform: none; }
}
```

## Responsive behavior

- `auto-fit` + `minmax()` collapses columns fluidly: ~6 across desktop,
  4 tablet, 2–3 mobile without media queries.
- Keep logo heights uniform (`height: 40px`) so rows align optically;
  let widths vary.
- On very narrow screens consider `grid-template-columns: repeat(2, 1fr)`
  and larger gaps to avoid cramped wordmarks.

## Accessibility

- Every `<img>` needs descriptive `alt` text — use the company name
  ("Acme Corp"), never "logo" or "image".
- If the wall is purely decorative and duplicated in nearby text, use
  `alt=""` instead so screen readers skip it.
- Wrap the grid in a labelled `<section>` or `<ul>` with a visible heading
  so context is announced.
- Grayscale is decorative only — never encode meaning through color state.
- Respect `prefers-reduced-motion`; disable the stagger and hover lift.
- Logos are not links here by default; if they become links, add a
  focus-visible outline matching your design system.

## Usage notes

- Serve SVGs where possible; request permission for third-party marks.
- Cap visible items at 8–12; link out to a full customer page beyond that.
