# Pricing Cards

A reusable 3-tier pricing section with a highlighted "recommended" plan,
feature checklists, and an accessible monthly/annual billing toggle.

## Structure

- Three equal-width cards in a responsive grid.
- Middle tier marked "Recommended" and visually emphasized.
- Each card: plan name, price, short description, feature checklist, CTA.
- Billing toggle switches all prices between monthly and annual values.

## Implementation sketch

```html
<section class="pricing" aria-labelledby="pricing-heading">
  <h2 id="pricing-heading">Pricing plans</h2>

  <div class="billing-toggle">
    <span id="billing-label">Billing period</span>
    <button role="switch" aria-checked="false" aria-labelledby="billing-label billing-state" id="billing-toggle">
      <span class="knob"></span>
    </button>
    <span id="billing-state">Monthly</span>
  </div>

  <div class="pricing-grid">
    <article class="pricing-card" aria-labelledby="plan-starter">
      <h3 id="plan-starter">Starter</h3>
      <p class="price"><span data-price data-monthly="9" data-annual="90">$9</span>/mo</p>
      <p class="description">For solo builders.</p>
      <ul class="features">
        <li>1 project</li>
        <li>Community support</li>
      </ul>
      <a href="/signup?plan=starter" class="cta">Choose Starter</a>
    </article>

    <article class="pricing-card recommended" aria-labelledby="plan-pro">
      <h3 id="plan-pro">Pro</h3>
      <p><span class="badge">Recommended</span></p>
      <p class="price"><span data-price data-monthly="29" data-annual="290">$29</span>/mo</p>
      <ul class="features">
        <li>Unlimited projects</li>
        <li>Priority support</li>
        <li>Custom domains</li>
      </ul>
      <a href="/signup?plan=pro" class="cta cta-primary">Choose Pro</a>
    </article>

    <article class="pricing-card" aria-labelledby="plan-team">
      <h3 id="plan-team">Team</h3>
      <p class="price"><span data-price data-monthly="79" data-annual="790">$79</span>/mo</p>
      <ul class="features">
        <li>Everything in Pro</li>
        <li>SSO &amp; audit logs</li>
      </ul>
      <a href="/signup?plan=team" class="cta">Choose Team</a>
    </article>
  </div>
</section>
```

```js
const toggle = document.querySelector("#billing-toggle");
const state = document.querySelector("#billing-state");

toggle.addEventListener("click", () => {
  const annual = toggle.getAttribute("aria-checked") !== "true";
  toggle.setAttribute("aria-checked", String(annual));
  state.textContent = annual ? "Annual (2 months free)" : "Monthly";

  document.querySelectorAll("[data-price]").forEach((el) => {
    el.textContent = `$${el.dataset[annual ? "annual" : "monthly"]}`;
  });
});
```

## Usage

1. Copy the markup into your pricing page; keep one `recommended` card.
2. Set `data-monthly` / `data-annual` per price; annual is billed once yearly.
3. Wire the CTA links to your signup flow with the plan in the query string.
4. Adjust tokens (`--accent`, spacing) via CSS custom properties on `.pricing`.

## Mobile behavior

Below `720px`, switch the grid from three columns to one so cards stack
vertically in DOM order (Starter, Pro, Team). The recommended card keeps its
emphasis via border/accent color — do not reorder it on mobile, since reading
order should match visual order. Keep the toggle full-width above the stack.

```css
.pricing-grid { display: grid; grid-template-columns: repeat(3, 1fr); gap: 1rem; }
@media (max-width: 720px) {
  .pricing-grid { grid-template-columns: 1fr; }
}
```

## Accessibility

- Heading hierarchy: single `h2` for the section, one `h3` per plan.
- Toggle uses `role="switch"` with `aria-checked`; label combines
  "Billing period" and the current state ("Monthly"/"Annual") via
  `aria-labelledby` so screen readers announce both.
- Feature lists are real `<ul>`/`<li>` elements, not styled paragraphs.
- The "Recommended" badge is supplementary text — the plan heading itself
  remains the accessible name of each card (`aria-labelledby`).
- Ensure the highlighted card's contrast still passes WCAG AA (4.5:1) and
  never rely on color alone: pair the accent border with the text badge.
- Keyboard: the toggle is a native `<button>`, reachable via Tab, toggled
  with Enter or Space; CTAs are links and inherit default semantics.
