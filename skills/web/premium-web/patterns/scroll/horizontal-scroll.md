# Horizontal Scroll

A section whose panels move horizontally while the user keeps scrolling
vertically. GSAP ScrollTrigger pins the section for a computed scroll
distance and translates the panel track on the X axis, so the viewport
slides sideways through a row of full-width panels.

## Anatomy

```html
<section class="h-scroll">          <!-- gets pinned by ScrollTrigger -->
  <div class="track" data-track>    <!-- display:flex, sized to panels -->
    <article class="panel">…</article>
    <article class="panel">…</article>
    <article class="panel">…</article>
  </div>
</section>
```

## Implementation sketch

```js
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";

gsap.registerPlugin(ScrollTrigger);

function initHorizontalScroll(section) {
  const track = section.querySelector("[data-track]");
  const getScrollAmount = () => track.scrollWidth - window.innerWidth;

  return gsap.to(track, {
    x: () => -getScrollAmount(),
    ease: "none",
    scrollTrigger: {
      trigger: section,
      start: "top top",
      end: () => `+=${getScrollAmount()}`,
      pin: true,
      scrub: 1,
      invalidateOnRefresh: true,
      anticipatePin: 1,
    },
  });
}
```

Key details:

- Pass `x` and `end` as functions plus `invalidateOnRefresh: true` so distances are recalculated on resize.
- `scrub: 1` adds catch-up smoothing; `scrub: true` gives strict 1:1 mapping.
- `anticipatePin: 1` prevents a visible jump when scrolling quickly into the section.

## Usage

1. Give the wrapper `height: 100vh` and the track `display: flex; width: max-content`.
2. Call `initHorizontalScroll(section)` after the DOM is ready.
3. Call `ScrollTrigger.refresh()` once late-loading images/fonts settle, or
   the pin distance is measured against stale dimensions.
4. Keep panels between 3–6; longer pin distances lose users' sense of place.

CSS essentials:

```css
.h-scroll { overflow: hidden; }
.track {
  display: flex;
  width: max-content;
  height: 100vh;
  will-change: transform;
}
.panel { flex: 0 0 100vw; height: 100vh; }
```

## Mobile fallback

Scroll-hijacked horizontal sections are frequently disabled or simplified on
touch devices: momentum scrolling fights the scrub, browser chrome
showing/hiding fires pin recalculations mid-gesture, and mobile users expect
vertical momentum to never be interrupted. Treat the pinned experience as a
desktop enhancement, not a requirement. Gate initialization behind a check:

```js
const isDesktop = window.matchMedia(
  "(min-width: 1024px) and (pointer: fine)"
).matches;

if (!isDesktop) return;
```

Stacked fallback on small screens:

```css
@media (max-width: 1023px), (pointer: coarse) {
  .track { display: block; width: auto; height: auto; }
  .panel { width: auto; height: auto; min-height: 60vh; }
}
```

An acceptable alternative when the sideways metaphor must survive on mobile
is native swiping — `overflow-x: auto` with
`scroll-snap-type: x mandatory` on the track — which keeps panels snapping
edge-to-edge without ever capturing the vertical gesture.

## Accessibility

Pinning removes the panels from normal document flow, so keyboard users must
be able to traverse them without the scroll gesture.

- Make each panel focusable (`tabindex="0"` when it holds no links or
  controls) and handle arrow keys on the section:

```js
section.addEventListener("keydown", (event) => {
  if (event.key !== "ArrowRight" && event.key !== "ArrowLeft") return;
  const panels = gsap.utils.toArray(".panel", section);
  const current = panels.findIndex((p) => p === document.activeElement);
  const next = event.key === "ArrowRight"
    ? Math.min(current + 1, panels.length - 1)
    : Math.max(current - 1, 0);
  panels[next].focus({ preventScroll: true });
});
```

- On focus, drive the tween to that panel's offset so sighted keyboard users
  land on the matching visual state (`tween.scrollTrigger.scroll(offset)`).
- Honor `prefers-reduced-motion`: skip pinning entirely and fall back to the
  same stacked layout used on mobile.
- Announce progress via an offscreen live region ("Panel 2 of 5") when panels
  are primarily visual, giving screen reader users equivalent orientation.
