# Stacking Cards

A reusable scroll pattern where each card pins in place while the next slides up over
it, forming a deck: earlier cards scale down slightly and stay put while later ones
stack on top. Built with GSAP ScrollTrigger pinning plus per-card scale/translate tweens.

## When to use

- Feature walkthroughs where each card represents one capability or step
- Pricing/plan reveals, onboarding flows, portfolio highlights
- Pages that need a sense of progression without navigation

Skip it for dense long-form content — pinned sections leave normal flow and hurt scanability.

## Implementation sketch

One wrapper section holding the cards in DOM order:

```html
<section class="stack" id="feature-stack">
  <article class="card">Card 01</article>
  <article class="card">Card 02</article>
  <article class="card">Card 03</article>
</section>
```

Cards get uniform sizing; ScrollTrigger owns the positioning:

```css
.card {
  min-height: 80vh;
  border-radius: 16px;
  background: #101014;
  color: #f4f4f5;
  will-change: transform;
}
```

Pin the section for the combined scroll distance, then scale each card down as its
successor arrives:

```js
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";

gsap.registerPlugin(ScrollTrigger);

const cards = gsap.utils.toArray("#feature-stack .card");

ScrollTrigger.create({
  trigger: "#feature-stack",
  start: "top top",
  end: () => "+=" + (cards.length - 1) * window.innerHeight,
  pin: true,
});

cards.forEach((card, i) => {
  const next = cards[i + 1];
  if (!next) return;
  gsap.to(card, {
    scale: 0.92,
    yPercent: -4,
    transformOrigin: "center top",
    ease: "none",
    scrollTrigger: {
      trigger: next,
      start: "top bottom",
      end: "top top",
      scrub: true,
    },
  });
});
```

Mechanics:

1. `pin: true` freezes the section at the viewport top for the computed distance.
2. Incoming cards are laid out normally, so they slide over the frozen predecessor.
3. `scale` with `transformOrigin: center top` makes covered cards recede, reading as depth.

## Usage instructions

- Give every card identical height (`min-height`); mismatched heights jump on release.
- Set explicit `z-index` per card position instead of relying on paint order.
- Set `anticipatePin: 1` to prevent a one-frame flicker on fast upward scrolls.
- Keep copy short: 3-5 cards is the sweet spot; past six, users lose orientation.
- Call `ScrollTrigger.refresh()` after fonts/images load so pin distance matches layout.

## Mobile fallback

Pinning fights mobile browsers' dynamic toolbars and momentum scrolling. Below the
breakpoint, skip pinning entirely and reveal cards sequentially:

```js
const mm = gsap.matchMedia();
mm.add("(min-width: 768px)", () => {
  // pin + scale setup from above
});
mm.add("(max-width: 767px), (prefers-reduced-motion: reduce)", () => {
  gsap.utils.toArray(".card").forEach((card) => {
    gsap.from(card, {
      y: 40,
      autoAlpha: 0,
      duration: 0.6,
      ease: "power2.out",
      scrollTrigger: { trigger: card, start: "top 85%" },
    });
  });
});
```

Same content order, no fixed positioning, no toolbar jitter.

## Performance notes

- Animate only `transform` and `opacity` (`autoAlpha`); never `top`/`margin`.
- Prefer `scrub: true` over a numeric scrub unless you deliberately want input lag.
- Limit `will-change` to `.card`; every promoted layer costs GPU memory.
- Honor `prefers-reduced-motion`: render cards statically instead of pinning.
- Profile while scrolling; jank usually means oversized layers — shrink cards or soften shadows on mobile.
