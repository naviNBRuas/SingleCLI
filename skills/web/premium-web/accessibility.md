# Accessibility for Premium, Highly-Animated Websites

Premium sites lean on scroll-hijacking, pinned sections, magnetic buttons,
custom cursors, and cinematic dark palettes — the techniques that break
assistive tech when applied naively. This guide keeps the polish while staying
usable for keyboard, screen-reader, and vestibular-sensitive users. Baseline:
WCAG 2.2 AA — not a separate "accessible mode", but the same experience built
on safer mechanisms from the start.

## 1. `prefers-reduced-motion` as an architecture decision

Do not sprinkle media queries over individual animations. Gate motion in one
place so every CSS transition, animation library, and JS effect reads the same
switch.

```css
@media (prefers-reduced-motion: reduce) {
  *,
  *::before,
  *::after {
    animation-duration: 0.01ms !important;
    animation-iteration-count: 1 !important;
    transition-duration: 0.01ms !important;
    scroll-behavior: auto !important;
  }
}
```

Use `0.01ms`, not `0s`: some browsers never fire `animationend` for
zero-duration animations, which breaks code listening for it. Keep end states
visible — fades must resolve to their final value (`animation-fill-mode:
both`) so content never stays invisible because an animation was skipped.

In JS (GSAP example), branch every timeline:

```js
const reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

if (reduced) gsap.set("[data-hero]", { y: 0, opacity: 1 });
else gsap.from("[data-hero]", { y: 60, opacity: 0, stagger: 0.08 });
```

Expose that check as a shared helper so every effect branches on one switch.

For ScrollTrigger pinning/parallax, wrap the setup in
`ScrollTrigger.matchMedia({ "(prefers-reduced-motion: no-preference)": ... })`
so pins exist only when motion is welcome — under reduced motion, content flows
in normal document order and pins disappear entirely.

Listen for live changes with `matchMedia(...).addEventListener("change", ...)`
so toggling OS-level reduced motion updates the running page without reload.

"Reduced" means: keep opacity crossfades and instant state changes; remove
parallax, scale/zoom, spin, marquee loops, autoplay video, and smooth-scroll
inertia. Movement conveying *state* may stay brief; decorative movement goes.

## 2. Keyboard navigation through scroll-hijacked / pinned sections

Scroll-jacking (Lenis, Locomotive, GSAP pinning) replaces native scrolling
with transformed containers. Two failure modes follow: `overflow: hidden`
traps keyboard focus, and screen readers announce content out of visual order.

- **Never trap Tab inside a pinned section.** If wheel input is hijacked until
  an animation completes, Enter/Space must also advance, and focus must then
  release to the next section.
- **Keep DOM order == visual order.** Build pinned "slides" as sequential
  siblings even if stacked visually via transforms; screen readers then read
  the narrative correctly without seeing the animation.
- **Provide a skip path**: a "skip section" link, or ensure the next landmark
  is reachable by standard Tab navigation.

Keyboard handler for a hijacked horizontal gallery:

```js
section.addEventListener("keydown", (e) => {
  if (e.key !== "ArrowRight" && e.key !== "ArrowLeft") return;
  e.preventDefault();
  panelIndex = clamp(panelIndex + (e.key === "ArrowRight" ? 1 : -1), 0, last);
  translateTrackTo(panelIndex);
  panels[panelIndex].focus({ preventScroll: true }); // panels: tabindex="-1"
});
```

Each panel needs `tabindex="-1"` for programmatic focus; the container gets
`role="group"`, `aria-roledescription="carousel"`, and an `aria-label`.
Announce slide changes through a visually hidden `aria-live="polite"` region
only when reduced motion is off — otherwise the carousel becomes a plain
vertical stack and the live region goes quiet. With Lenis or similar, respect
reduced motion by not instantiating the smooth scroller at all.

## 3. Focus states on magnetic buttons and custom cursors

Custom cursor implementations usually do `cursor: none` globally — hiding the
pointer for keyboard users who rely on it and killing the text I-beam:

- Scope `cursor: none` to `(pointer: fine)` **and** non-reduced-motion
  contexts; touch and reduced-motion users keep the native cursor.
- Swap only after the custom cursor's first confirmed render (post-
  `mousemove`), so a failed script never leaves a blank pointer.
- Mark the custom cursor `aria-hidden="true"` with `pointer-events: none`.

Magnetic buttons translate toward the pointer on hover; trouble starts when
the hit target moves or focus styling is an afterthought. Keep the moving
element as an inner span of a static anchor, and put the ring on the static
parent — it never drifts:

```css
.magnetic:focus-visible {
  outline: 2px solid var(--accent);
  outline-offset: 4px;
}
```

Drive magnetism from `mousemove` on the button itself; reset transform to
identity on both `mouseleave` and `blur`, or tabbing away leaves the button
stranded offset from its label. Under reduced motion skip translation
entirely; hover/focus feedback falls back to color or underline changes.

Every interactive element needs visible `:focus-visible` styling with ≥3:1
contrast against adjacent colors (WCAG 2.2 Focus Appearance). Never ship
`outline: none` without a replacement. For split-text headline links where
per-character spans fragment the accessible name, set `aria-hidden="true"` on
the animated spans and put an `aria-label` on the anchor.

## 4. Landmarks and heading structure

Animated sites often render everything as divs inside canvas-like layouts,
erasing semantics into an unnavigable sea of identical regions.

- One `<main>` per page; `<header>`/`<footer>` landmarks; `<nav
  aria-label="Primary">` for menus. Overlay menus need `aria-expanded` on the
  trigger and `inert` (or `visibility: hidden`) while closed, so links are
  never tabbable invisibly.
- Headings form a logical outline (`h1 → h2 → h3`) even when styled as
  display-type art. A 12vw kinetic headline is an `<h2>` in its section, not a
  `<div class="title">`.
- Decorative layers — grain overlays, gradient meshes, canvas/video
  backgrounds — get `aria-hidden="true"` and `tabindex="-1"`.
- Wrap each full-screen scroll scene in `<section aria-labelledby>` pointing
  at its heading, so screen-reader users can jump between scenes via the
  headings list, bypassing the choreography.
- Page transitions (barba/swup-style): after swap, move focus to the new
  view's `h1` or main container and update `document.title`; otherwise screen
  readers keep announcing the dead page.
- Cinematic modals still need `role="dialog"`, `aria-modal="true"`, focus
  moved in on open, restored on close, Escape to dismiss.

## 5. Color contrast in dark cinematic palettes

Dark themes fail contrast more often than light ones because designers reach
for muted grays-on-black. WCAG requires 4.5:1 body text; 3:1 for text ≥24px
(18.66px bold), UI components, and graphical objects.

- Body text on `#000`–`#111`: minimum gray around `#9CA3AF` (~7:1). Anything
  below ~`#767676` on black fails — that is the classic 4.5:1 boundary.
- Low-emphasis metadata (captions, labels, "©2026") is still body text to its
  readers; the large-text exemption is about size, not importance. Audit it.
- Test accents against the dark background you ship, not white: deep purples
  and reds that pass on white often fail on black. Neon lime/cyan/magenta
  clear 7:1 easily — prefer shifting luminance over hue when fixing.
- Text over footage needs enforcement, not luck: add a scrim sized to the
  text block's worst-case frame, and test against the loop's brightest frame,
  not the poster image.
- Non-text indicators get 3:1 too: focus rings, form borders, icon-only
  buttons, chart strokes (1.4.11).
- Never encode state in color alone — pair errors with icons/text, active nav
  with weight or underline.
- Verify with axe DevTools plus manual spot checks; automation catches static
  pairs but not text-over-video cases.

## Pre-launch checklist

- Reduced motion removes parallax, pins, marquees, autoplay video; page fully
  readable in that state
- Keyboard-only operation works end to end; no trapped focus; visible rings;
  magnetic buttons reset on blur; cursor fallback intact for touch/reduced
- Single h1, logical heading order, one main landmark; overlays/modals handle
  `aria-expanded`, `inert`, focus restore, and Escape correctly
- All text ≥4.5:1 (≥3:1 large), including over-video scrims; UI ≥3:1
- Page transitions move focus and update title; axe clean plus a manual
  VoiceOver/NVDA pass on home and one pinned case
