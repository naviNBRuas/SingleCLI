# Pattern: Pinned / Sticky Scroll Section

Pin a section to the viewport while its inner content animates through
scrubbed states, then release it once the sequence completes. This is the
signature "scrollytelling" effect used across premium marketing sites.

## When to use

- A step-by-step walkthrough (feature tour, process diagram)
- A hero that transitions through 2–4 visual states before unpinning
- Horizontal scroll galleries driven by vertical scrolling

## Implementation sketch

```html
<section class="pinned" data-pin>
  <div class="pinned__track">
    <div class="pinned__panel">Panel 1</div>
    <div class="pinned__panel">Panel 2</div>
    <div class="pinned__panel">Panel 3</div>
  </div>
</section>
```

```js
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";

gsap.registerPlugin(ScrollTrigger);

const reduceMotion = window.matchMedia(
  "(prefers-reduced-motion: reduce)"
).matches;

if (!reduceMotion && window.matchMedia("(min-width: 768px)").matches) {
  const panels = gsap.utils.toArray(".pinned__panel");

  const tween = gsap.to(panels, {
    xPercent: -100 * (panels.length - 1),
    ease: "none",
    scrollTrigger: {
      trigger: "[data-pin]",
      pin: true,
      scrub: 1,
      start: "top top",
      end: () => "+=" + document.querySelector("[data-pin]").offsetWidth * (panels.length - 1),
      anticipatePin: 1,
      invalidateOnRefresh: true,
    },
  });
}
```

Key options:

- `pin: true` wraps the trigger in a pin-spacer so layout below shifts down.
- `scrub` ties tween progress to scroll position; `scrub: 1` adds easing lag.
- `anticipatePin: 1` pre-pins slightly to avoid a visible jump on fast scrolls.
- `invalidateOnRefresh` recalculates `end` values after resize/orientation change.

## Usage instructions

1. Give the section an explicit height (e.g. `min-height: 100vh`) — pinning a
   zero-height element produces an invisible pin-spacer.
2. Avoid `overflow: hidden` on ancestors of the pinned element; ScrollTrigger
   measures offsets against the scroller and clipped ancestors break it.
3. Call `ScrollTrigger.refresh()` after fonts/images load or after any dynamic content insertion above the section.
4. Keep one pinned section per viewport at a time; overlapping pins fight for scroll space and produce jitter.

## Mobile fallback

Disable pinning on small screens (`matchMedia("(min-width: 768px)")` gate
above). Reasons:

- Mobile browsers collapse/expand the URL bar during scroll, firing repeated
  viewport resizes; each resize forces `refresh()` and visibly re-jumps the
  pinned element mid-sequence.
- iOS Safari composites fixed/sticky elements on a separate layer, which
  causes flicker and rubber-band tearing while scrubbing.
- Pin-spacer divs inflate page height dramatically, making momentum scrolling
  feel heavy and draining battery on low-end devices.

Alternative: replace pinning with plain CSS `position: sticky`. Let panels
stack normally (vertical list or native horizontal `scroll-snap` carousel) and
keep only a lightweight opacity/scale transition via IntersectionObserver:

```css
.pinned__track { display: block; }
.pinned__panel { min-height: 80vh; }
```

The content stays fully readable without JS-driven scroll hijacking, and the
narrative survives even where the effect does not.

## Accessibility

- Gate all pin logic behind `prefers-reduced-motion: reduce`; reduced-motion
  users get the static stacked layout instead of scrubbed animation.
- Pinned elements are moved into pin-spacers but stay in DOM order, so tab and
  screen-reader flow is preserved — verify focus order manually after adding
  transforms, since `transform` creates a new containing block for
  `position: fixed` children (move tooltips/modals out of the pinned subtree).
- Ensure every interactive control inside the section is reachable without
  scrolling being hijacked: keyboard users must not get trapped because
  `scrub` ignores wheel-free navigation. Provide skip links around long pinned
  sequences.
- Panels hidden off-screen via transform remain in the accessibility tree;
  set `aria-hidden="true"` plus `inert` on non-active panels and toggle both
  from ScrollTrigger's `onUpdate` callback so screen readers announce only the
  visible panel.
