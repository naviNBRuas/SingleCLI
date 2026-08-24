# Comparison Table — Premium Pricing Pattern

A reusable pricing/comparison table for plan tiers. Features a sticky header row
during scroll, a highlighted recommended tier column, and subtle hover feedback
on feature rows. Built with real `<table>` semantics — no div grids pretending
to be tables.

## Structure sketch

```html
<section class="pricing" aria-labelledby="pricing-heading">
  <h2 id="pricing-heading">Compare plans</h2>
  <div class="table-scroll" role="region" aria-label="Plan comparison" tabindex="0">
    <table class="compare-table">
      <thead>
        <tr>
          <th scope="col"><span class="visually-hidden">Feature</span></th>
          <th scope="col">Starter</th>
          <th scope="col" class="tier--recommended">
            <span class="badge">Recommended</span> Pro
          </th>
          <th scope="col">Enterprise</th>
        </tr>
      </thead>
      <tbody>
        <tr class="price-row">
          <th scope="row">Price</th>
          <td>$9/mo</td>
          <td>$29/mo</td>
          <td>Contact us</td>
        </tr>
        <tr>
          <th scope="row">Projects</th>
          <td>3</td><td>Unlimited</td><td>Unlimited</td>
        </tr>
        <tr>
          <th scope="row">Support</th>
          <td>Email</td><td>Priority</td><td>Dedicated CSM</td>
        </tr>
      </tbody>
    </table>
  </div>
</section>
```

## CSS implementation sketch

```css
.compare-table {
  width: 100%;
  border-collapse: separate;
  border-spacing: 0;
}

/* Sticky header row while scrolling vertically */
.compare-table thead th {
  position: sticky;
  top: 0;
  z-index: 2;
  background: var(--surface);
  backdrop-filter: blur(8px);
  box-shadow: inset 0 -1px 0 var(--border-subtle);
}

/* Highlighted recommended tier */
.tier--recommended {
  color: var(--accent-fg);
}
.tier--recommended .badge {
  display: block;
  font-size: 0.75rem;
  text-transform: uppercase;
  letter-spacing: 0.05em;
}

/* Subtle hover highlight per feature row */
.compare-table tbody tr:hover {
  background: var(--surface-hover);
  transition: background-color 120ms ease;
}

/* Mobile: horizontal scroll wrapper instead of breaking layout */
.table-scroll {
  overflow-x: auto;
  -webkit-overflow-scrolling: touch;
}
```

## Usage instructions

1. Wrap the table in `.table-scroll` so narrow viewports pan horizontally.
2. Mark exactly one column with `.tier--recommended`; keep it second or third,
   not last, so the badge reads as a nudge rather than an upsell wall.
3. Use `scope="col"` on header cells and `scope="row"` on feature names — this
   is what makes screen readers announce "Projects, Pro, Unlimited".
4. Keep row labels in `<th scope="row">`, values in `<td>`; never swap them.
5. Set a sensible `min-width` on the table (e.g. `640px`) so columns don't
   crush before scrolling kicks in.

## Mobile behavior

- Default: horizontal scroll inside the wrapper, with a visible scroll affordance
  (fade gradient on the right edge).
- Alternative under `max-width: 480px`: switch to stacked cards via
  `display: grid` on rows — each plan becomes a card listing its features.
- If stacking, duplicate the plan name into every card's heading and hide the
  original `<thead>` visually (`clip-path`), keeping it for assistive tech.

## Accessibility notes

- Real table elements only: `<table>`, `<thead>`, `<tbody>`, `<th scope>`.
- The scroll wrapper needs `role="region"` + `aria-label` + `tabindex="0"`
  so keyboard users can reach and scroll it.
- Hover highlights are decorative only — never rely on hover to convey state;
  pair any selected tier with `aria-selected` on the corresponding control.
- Respect `prefers-reduced-motion`: drop the transition on row hover.
