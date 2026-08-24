# Motion Design for Premium Websites

Motion is not decoration — it is choreography. On a premium site, every
animated element should either orient the user (where am I, what changed),
direct attention (look here next), or communicate cause and effect. If an
animation does none of these, cut it. This document covers the standard stack:
**GSAP 3** with **ScrollTrigger**, **Lenis**, and the discipline that keeps
motion feeling expensive rather than busy.

## 1. GSAP + ScrollTrigger fundamentals

Import `gsap` and `ScrollTrigger` from their packages and call
`gsap.registerPlugin(ScrollTrigger)` once at boot. Core tween methods:
`gsap.to()`, `gsap.from()`, `gsap.fromTo()`, and `gsap.set()` (instant, no
animation). Compose them into sequences with
`gsap.timeline({ defaults: { ease: "power3.out" } })` so shared settings live
in one place instead of being repeated per tween.

Animate **only `transform` and `opacity`** wherever possible — these run on
the compositor. Animating `top`, `left`, `width`, `height`, or `margin`
forces layout every frame and stutters on mid-range phones; when size must
change, prefer `scaleX`/`scaleY` or FLIP-style techniques.

ScrollTrigger attaches to any tween via a `scrollTrigger` config object:
`gsap.to(".hero-title", { y: 0, opacity: 1, scrollTrigger: { trigger:
".hero", start: "top 80%" } })` fires when `.hero`'s top crosses 80% down
the viewport. Key concepts:

- **`trigger` + `start` + `end`** define *when* in scroll space something
  happens. `"top top"` aligns with the viewport top; percentages are relative.
- **`scrub: true`** (or a number like `scrub: 1` — seconds of smoothing) ties
  progress directly to scrollbar position instead of playing once.
- **`pin: true`** holds the trigger element fixed while its `end` scrolls by.
  Powerful but expensive; see the budget section.
- **`toggleActions`** controls enter/leave/enter-back/leave-back behavior;
  order is play/pause/reverse/reset. Callbacks (`onEnter`, `onLeaveBack`,
  `onUpdate(self => self.progress)` etc.) cover everything event-driven.
- After DOM changes that alter page height (images loading, accordions),
  call `ScrollTrigger.refresh()`. For layout-dependent values use
  `invalidateOnRefresh: true` and compute inside functions:
  `y: () => -window.innerHeight * 0.5`.

Batch-reveal repeated elements by looping over `gsap.utils.toArray("[data-reveal]")`
or with `ScrollTrigger.batch()` — never hand-write one tween per element.

## 2. Lenis smooth scroll

Lenis replaces native scrolling with a smoothed virtual scroll. The canonical
GSAP wiring drives Lenis off `gsap.ticker` so both share one rAF loop — never
run two independent rAF loops:

```js
import Lenis from "lenis";

const lenis = new Lenis({
  duration: 1.2,
  easing: (t) => Math.min(1, 1.001 - Math.pow(2, -10 * t)),
  smoothWheel: true,
});

lenis.on("scroll", ScrollTrigger.update);

gsap.ticker.add((time) => {
  lenis.raf(time * 1000);
});

gsap.ticker.lagSmoothing(0);
```

Rules of thumb: keep `duration` modest (roughly 0.9–1.4s — longer feels like
syrup and delays user intent); use `lenis.scrollTo(target, { offset: -80 })`
for anchor links instead of raw `scrollIntoView`; call `lenis.stop()` /
`lenis.start()` around modals and fullscreen menus, making sure pinned
elements live inside Lenis' scrolled container or they will visibly detach;
destroy the instance (`lenis.destroy()`) on SPA route teardown; and skip
construction entirely when the user prefers reduced motion (see §6).

## 3. Choreographing scroll-driven timelines

The signature premium move is a **pinned, scrubbed master timeline**: the
section pins, and scroll position scrubs through a multi-step sequence.

```js
const tl = gsap.timeline({
  scrollTrigger: {
    trigger: ".showcase",
    start: "top top",
    end: "+=250%",          // three viewport-heights of runway
    scrub: 1,
    pin: true,
    anticipatePin: 1,
  },
});

tl.from(".device", { scale: 0.85, rotateX: -8 })
  .to(".headline", { opacity: 0, yPercent: -30 }, "<10%")
  .fromTo(".feature-a", { xPercent: 100 }, { xPercent: 0 })
  .to({}, { duration: 0.25 })   // deliberate beat — let the frame rest
  .fromTo(".feature-b", { yPercent: 60 }, { yPercent: 0 });
```

Choreography principles:

- Position parameters (`"<"`, `"<10%"`, `">+0.2"`) make timing relative and
  editable. Absolute durations scattered across tweens are unreviewable.
- Insert **beats of stillness** between movements (`tl.to({}, { duration: x })`).
  Continuous motion reads as cheap; motion–rest–motion reads as intentional.
- Overlap entrances slightly (each starting ~80% through the previous) for
  flow, but never have five things arriving at once.
- Scrubbed timelines must be **reversible-safe**: avoid `.set()` mid-timeline
  and one-shot callbacks, because users will scroll back up. Test pinned
  sections at multiple viewport heights — short laptops shrink your runway,
  so prefer percentage-based `end` values over pixels.

## 4. Micro-interactions vs. cinematic transitions

Keep two distinct tiers, and never blur them.

**Micro-interactions** (buttons, inputs, cards, hovers): 150–400ms durations;
`power2.out` for exits, `power3.out` or `back.out(1.4)` for playful entrances;
small distances (2–8px lifts, subtle `scale(1.02)` max on hover). Drive these
with CSS transitions where possible; reach for GSAP only when you need
sequencing or interruption control (overwrite-aware hover tweens). Whatever
the mechanism, respond within ~100ms of input or the UI feels dead.

**Hero-level cinematic moments** (load-in, pinned showcases, section
transitions): 0.8–2s total, eased with `power3.inOut` or `expo.out`. One clear
protagonist per moment, supporting elements staggering behind it
(`stagger: { each: 0.08 }`) — never arriving simultaneously. Load-in should
let the user act within ~1.5s; gate long intros behind a skip-on-interaction
rule. Reserve full-screen wipes, pinned scrollytelling, and parallax stacks
for at most two or three set-piece moments per page — a site where every
section is a spectacle has no hierarchy and reads as a template.

## 5. The animation budget

Treat motion as a finite resource with an explicit budget:

- **Max 1 major scroll-driven animation active per viewport** — one pinned
  sequence or one scrubbed scene. Never two pinned regions competing.
- **Max ~3 simultaneous major tweens** within any single choreographed moment;
  everything else staggers before or after. Keep concurrently animated
  elements under ~20 — beyond that, batch reveals via ScrollTrigger so only
  visible elements animate.
- Every animation must declare its purpose in review: *orient, direct, or
  explain*. Motion with no stated purpose gets deleted, not debated.
- Performance floor: 60fps on a mid-range phone, profiled with CPU throttling.
  If jank appears, first stop hoarding `will-change` (apply via
  `gsap.set(el, { willChange: "transform" })` during animation and clear it in
  `onComplete`), then cut effects, then reduce durations.
- Pinning costs: each `pin` adds a spacer and repaint pressure. Budget at most
  one pinned section per screen-height of page, and consider disabling pin
  below `768px`.

When a stakeholder asks for one more animated flourish, the budget is the
answer: name what it displaces.

## 6. Reduced-motion fallback strategy

Reduced motion is accessibility, not optional polish. Honor
`prefers-reduced-motion: reduce` everywhere. GSAP's built-in mechanism is
`gsap.matchMedia()`, which scopes animations by media query and auto-cleans
on change:

```js
const mm = gsap.matchMedia();

mm.add("(prefers-reduced-motion: no-preference)", () => {
  buildCinematicIntro();   // timelines, pins, scrubbed scenes
});

mm.add("(prefers-reduced-motion: reduce)", () => {
  gsap.set("[data-reveal]", { opacity: 1, y: 0 }); // final states only
});
```

Strategy rules:

1. **Content parity.** The reduced experience shows the same information and
   reaches the same final states — instantly or via opacity-only fades ≤200ms.
2. **No scrubbing, no pinning, no parallax** under reduced motion. Replace
   pinned scrollytelling with static stacked layouts (clean if the pin is only
   created inside the `no-preference` context).
3. **Smooth scroll off.** Don't construct Lenis when reduced motion is
   preferred; native scroll is the fallback.
4. **Apply initial-hidden states via JS/GSAP** (`gsap.from()`, `gsap.set()`),
   never baked into CSS as `opacity: 0`. If CSS hides content and JS fails,
   content disappears — a correctness bug motion review should catch.
5. **Test the toggle.** Flip reduce-motion in OS settings mid-session;
   `gsap.matchMedia()` contexts must tear down without leaving orphaned
   ScrollTriggers (verify with `ScrollTrigger.getAll()`).

---

## Checklist before shipping

- [ ] Only `transform`/`opacity` animated (audit for layout properties).
- [ ] One rAF loop total (Lenis driven by `gsap.ticker`).
- [ ] ≤1 major scroll animation per viewport; ≤3 concurrent major tweens.
- [ ] Set-pieces per page ≤3; micro-interactions stay 150–400ms.
- [ ] `gsap.matchMedia()` guards all nonessential motion; final states reachable without it.
- [ ] `ScrollTrigger.refresh()` after late layout shifts; 60fps verified under CPU throttling.
