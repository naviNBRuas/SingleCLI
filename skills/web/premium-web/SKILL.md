---
name: premium-web
description: Build premium, art-directed websites with deliberate visual hierarchy, composition, typography, depth, and motion. Use when creating landing pages, marketing sites, portfolios, or any interface where design quality is the product.
---

# Premium, Art-Directed Web Design

Premium sites are not decorated templates. Every pixel, every millisecond of motion,
and every breakpoint decision is intentional. This skill encodes the judgment calls
that separate a designed page from an assembled one.

The core principle: **decide what matters most on each screen, then make everything
else visibly subordinate to it.** Most "AI-looking" pages fail because ten elements
compete for attention at equal volume.

## Visual Hierarchy

Before writing CSS, rank the page's elements by importance:

1. **Primary** — the one thing a visitor must see or do (headline, hero product,
   primary CTA). It gets the largest scale, strongest contrast, and first fixation.
2. **Secondary** — supporting content that explains the primary (subheadings, feature
   blocks, imagery).
3. **Tertiary** — wayfinding and metadata (nav links, footnotes, legal text).

Make the ranking legible through at least two simultaneous signals: size **and**
weight/color/position. One signal alone (e.g., only slightly bigger text) reads as
accidental. If two elements feel like they're the same importance, either commit to a
real difference or make them truly identical — ambiguity is what looks cheap.

Sanity check: squint at the page (or blur it in devtools). You should still be able
to name the primary element instantly. If you can't, increase separation.

## Composition

Composition is where the eye travels, in what order, and where it rests.

- **Focal point**: exactly one per viewport. Secondary focal points are allowed only
  when separated by scroll distance.
- **Asymmetry over centering**: symmetric centered layouts flatten everything.
  Offset key content off-axis, use asymmetric grids (e.g., 5/7, 4/8 splits), and let
  whitespace carry weight. Centered layouts are fine when *symmetry itself* is the
  statement (formal, ceremonial) — use them deliberately, not by default.
- **Optical alignment beats mathematical alignment**: text next to icons should align
  optically; a circle needs slight overhang against a flat edge to look aligned.
- **Rhythm and repetition**: repeat alignment axes, corner radii, and image aspect
  ratios so the page feels composed rather than tiled. Break your own pattern once,
  deliberately, for emphasis — never accidentally.
- **Negative space is content**: if a section feels cramped, the fix is removing
  elements or increasing space, never shrinking gaps uniformly.

## Spacing Rhythm

Use one spacing scale (e.g., 4px base: 4/8/12/16/24/32/48/64/96/128) and never
invent off-scale values. Then apply it hierarchically:

- **Spacing between sections > spacing between cards > spacing within a card >
  spacing between related words.** Proximity must mirror relationship.
- Section padding typically 96–160px desktop; card padding 24–48px; component
  internals 8–16px.
- Related items belong close together; unrelated items need generous separation.
  When grouping fails on a page, it's almost always under-spacing between groups,
  not over-spacing inside them.
- Vertical rhythm: pick a line-height unit (e.g., 8px) and snap block margins to it
  so text columns breathe consistently.

## Typography Hierarchy

Establish a type scale before styling anything else — type is the skeleton of the
page.

- **Scale with intent**: e.g., 1.25 ratio (12/16/20/25/31/39/49/61). Hero headlines
  often justify larger jumps (clamp() fluid sizing from ~2.5rem to ~5rem+).
- **Limit to two families maximum** (often one display + one body), three weights
  per family in most UIs. Every additional font is another voice competing in the
  room.
- **Line length**: 45–75 characters for body text (~60ch max-width). Long lines kill
  readability faster than small fonts do.
- **Line height scales inversely with size**: body ~1.5–1.6, headings ~1.05–1.2,
  display can go ~0.95–1.0 with negative letter-spacing (-0.02em to -0.03em on large
  sizes). Large tight, small loose.
- **Hierarchy via more than size**: combine weight (400 vs 600 vs 700), case
  (uppercase micro-labels with wide tracking), and color (muted secondary text).
  A 13px uppercase 0.08em-tracked label above a heading creates structure that pure
  sizing cannot.
- Body text color is rarely pure black — near-black (#111–#333 range) with adequate
  contrast reads as more refined than #000.

## Contrast

Contrast directs attention; uniform contrast means no direction.

- **Functional contrast first**: WCAG AA minimums are non-negotiable — 4.5:1 body
  text, 3:1 large text (≥24px / 18.66px bold) and UI boundaries. Check actual
  computed values, not vibes.
- **Aesthetic contrast second**: dark-on-light or light-on-dark inversions for whole
  sections create chapter breaks in long pages. A dark hero flowing into a light
  features section gives each zone identity.
- **Mute the supporting cast**: secondary text at reduced opacity/color, borders at
  low-alpha, hover states that brighten. Reserve full-strength color for interactive
  and primary elements so they read as clickable.
- Color accent discipline: one dominant accent color, used sparingly (CTAs, links,
  highlights). If accent color covers more than ~10% of the viewport, it stops
  signaling anything.
- Contrast is also structural: pair large/light with small/dark, rough with smooth,
  dense with sparse. Pages made only of mid-tones and medium sizes feel beige even
  when colorful.

## Depth and Layering

Flat design is a choice; premium flat design still implies space.

- **Elevation system**: define 3–5 consistent shadow levels (resting card, raised
  card/hover, dropdown, modal, toast) and reuse them. Shadows must share a hue with
  ambient light (usually desaturated blue-black), never pure black.
- **Shadows communicate physics**: larger blur + larger offset + lower opacity =
  higher elevation. A dropdown shadow should be softer than its trigger's hover
  shadow.
- Layer with overlap deliberately: images breaking out of their grid cell, badges
  overlapping card edges, sticky nav casting shadow only after scroll. Overlap is
  one of the cheapest ways to escape "template" appearance.
- Parallax and fixed backgrounds: use at most once per page and only when the layer
  shift reinforces spatial meaning (background recedes, foreground advances).
- Glass/blur effects (backdrop-filter): reserve for floating chrome over variable
  content (navs, players). Ensure fallbacks for browsers without support — a blurred
  panel that degrades to solid is fine; one that degrades to transparent is broken.

## Motion Hierarchy

Motion is information. Every animation must answer: *what does this tell the user?*
Valid answers:

- **Hierarchy**: entrance sequences reveal primary → secondary → tertiary, teaching
  the eye the reading order. Stagger siblings by 40–80ms, not 300ms.
- **State**: hover/focus/press feedback, loading → loaded, error shake, toggle flips.
  State changes animate so users see cause and effect.
- **Spatial relationships**: modals scale from their trigger button; panels slide
  from the edge they live on; a detail view expanding from the card it came from.
  Motion explains where things come from and go.
- **Navigation**: route transitions preserve context — shared-element transitions,
  directional slides (forward = leftward), persistent nav that doesn't re-animate.
- **Progression**: multi-step forms sliding between steps, progress bars filling,
  skeletons resolving into content. Motion shows movement through a flow.

If an animation answers none of these, it's decoration. Delete it.

**Not everything animates simultaneously — this rule is absolute.** When multiple
elements animate at once with similar timing/distance/direction, the result reads as
chaos or as a loading glitch. Rules:

- Sequence entrances: hero first, then supporting elements staggered, then chrome.
  Total entrance choreography should complete in under ~800ms.
- While something is entering, nothing nearby should be exiting or looping.
- Continuous loops (ambient float, gradient shimmer, marquee) are limited to ONE per
  viewport, low-amplitude, and never on the primary element.
- Interaction feedback (hover/press) is exempt from sequencing but must be fast:
  100–200ms ease-out. Anything slower makes the UI feel laggy.

Timing guidance: micro-interactions 100–200ms; element transitions 250–400ms;
page-level choreography up to 600–800ms. Use standard easing curves — ease-out for
entrances, ease-in for exits, ease-in-out for moves between known points. Springy/
overshooting curves sparingly, for playful brands only.

## Responsive Strategy

Responsive is not "desktop, but smaller." Each breakpoint is an art-direction
decision about what content serves users on that device.

- **Mobile ≠ shrunken desktop**: reflow asymmetric grids to stacked single-column,
  but re-order content by mobile priority (CTA earlier, decorative imagery later or
  cut entirely). DOM order may need to differ from visual order — handle with care
  for screen-reader logic.
- **Substitutions, not just scaling**: replace horizontal scrollers with vertical
  stacks; replace hover tooltips with tap-revealed details; swap wide tables for
  definition lists; replace multi-column feature grids with carousels OR collapse
  to top-N items. Some compositions genuinely don't survive translation — redesign
  them instead of cramming.
- **Typography rescales non-linearly**: use clamp(min, preferred, max). Headlines
  compress harder than body text. A 64px headline becoming 28px on mobile is fine;
  body text dropping below 16px is not.
- **Touch targets ≥44×44px**, spacing between targets ≥8px. Hover-only affordances
  must have touch/keyboard equivalents.
- **Performance IS responsive design**: ship smaller images (srcset/sizes, AVIF/WebP),
  lazy-load below-fold media, avoid layout-shifting animations. A beautiful site
  that takes 6s on mid-range mobile is not premium.
- Test at real breakpoints (360, 768, 1024, 1440, 1920+) AND odd widths (390, 820)
  where grids commonly break.

## Accessibility

Accessibility is a floor, not a feature — and several premium techniques silently
break it if unchecked.

- **`prefers-reduced-motion` is REQUIRED.** Gate every animation and transition:

  ```css
  @media (prefers-reduced-motion: reduce) {
    *, *::before, *::after {
      animation-duration: 0.01ms !important;
      animation-iteration-count: 1 !important;
      transition-duration: 0.01ms !important;
      scroll-behavior: auto !important;
    }
  }
  ```

  In JS-driven animation (scroll-triggered reveals, parallax), check
  `matchMedia('(prefers-reduced-motion: reduce)')` and render the final state
  immediately. Content must never be hidden behind an animation that never plays.
- **Graceful degradation**: every effect must have a sensible no-JS/no-support
  state. Scroll-triggered reveals default to visible when JS fails. Fonts fall back
  to metric-compatible stacks. Layout works without custom properties.
- Semantic HTML first: real headings in order, landmarks, buttons for actions,
  labels for inputs. Art direction happens ON TOP of semantics, never instead of.
- Keyboard operability: visible focus states (design them — offset outlines, brand-
  colored rings), logical tab order, no keyboard traps in carousels/menus, skip link
  on long pages.
- Text over imagery requires a contrast-checked scrim behind the text, not hope.
- Don't remove focus outlines without replacing them; don't rely on color alone to
  convey state; caption or aria-label meaningful media.

## Self-Check: Would a Professional Designer Approve?

Before shipping, answer honestly. Any "no" or "unsure" means iterate.

1. Can I name the single most important element per viewport in one glance?
2. Squinting at the page, does the hierarchy still read?
3. Does every animation communicate hierarchy/state/space/navigation/progress?
   List them; delete any that don't.
4. Is anything animating simultaneously that shouldn't be? More than one ambient
   loop on screen?
5. Are all spacing values from one scale? Do proximity groups match relationships?
6. Is body copy 45–75ch with ≥4.5:1 contrast? Headline/body contrast obvious?
7. Do shadows/elevation follow one consistent physical model?
8. At 390px and 1920px, did we art-direct or merely shrink? Any hover-only
   functionality on touch?
9. With reduced-motion enabled, is everything visible and usable immediately?
10. With JS disabled, does content remain readable (even if unstyled extras)?
11. Is there ONE accent color doing the signaling, or has it spread everywhere?
12. Would this pass as a deliberate composition — asymmetry, negative space, one
    focal point — or does it look like components stacked in order?
