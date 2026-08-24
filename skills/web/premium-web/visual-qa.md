# Visual QA for Premium Websites

A scoring rubric and self-check protocol for judging whether a site meets
premium standards — award-studio polish, not template-marketplace output.

Score each category **0–10**; anything below **7** is a blocking issue.
A premium verdict requires **63+/70** with no single category under 7.

---

## Scoring Rubric

| Category       | Weight | Pass |
|----------------|--------|------|
| Hierarchy      | 0–10   | ≥7   |
| Typography     | 0–10   | ≥7   |
| Motion         | 0–10   | ≥7   |
| Composition    | 0–10   | ≥7   |
| Mobile         | 0–10   | ≥7   |
| Accessibility  | 0–10   | ≥7   |
| Performance    | 0–10   | ≥7   |

---

## 1. Visual Hierarchy (0–10)

Does the eye travel through the page in the intended order?

- One unmistakable focal point per viewport; competing focal points fail.
- Size, weight, color, and spacing are used deliberately — not uniformly.
- Primary CTA is visually dominant; secondary actions are quieter.
- Whitespace groups related content; section rhythm varies so no two
  sections share the same internal layout.
- Squint test: at low fidelity, the page still reads in the right order.

**Deduct for**: flat sameness, decorative elements outranking functional ones.

## 2. Typography (0–10)

Type is the largest surface area of most premium sites.

- A deliberate type scale (e.g., modular scale), not arbitrary sizes.
- Line length held near 45–75 characters for body text.
- Line-height tuned per size: tighter for display, looser for body.
- Optical alignment on edges; hanging punctuation where appropriate.
- Letter-spacing adjusted: negative for display, positive for small uppercase labels.
- Maximum two type families; weights used intentionally.
- No orphaned words or awkward rag in headings and pull quotes.

**Deduct for**: default browser type, inconsistent vertical rhythm,
centered long-form text.

## 3. Motion (0–10)

Motion communicates state and structure; it never decorates for its own sake.

- Every animation answers "what changed?" — entrance, exit, or transition.
- Custom-feeling easings (ease-out-quart/expo); never raw `linear`
  or default CSS easing on UI transitions.
- Durations in the 150–600ms band for micro-interactions; longer only
  for narrative scroll moments.
- Scroll-linked motion reveals content in sync with reading position.
- Hover states respond within 100ms and feel physical, not binary.
- Reduced-motion variants (`prefers-reduced-motion`) are genuinely calmer, not instant.
- Infinite loops only for subtle, genuine ambient elements.

**Deduct for**: parallax everywhere, fade-up on every element,
spinners over skeletons.

## 4. Composition (0–10)

The layout should feel authored, not assembled from blocks.

- Grid discipline — visible alignment, then purposeful breaks for emphasis.
- Asymmetry and negative space used confidently — not every section is centered stacked cards.
- Imagery is art-directed: chosen crops, consistent treatment, no stock feel.
- Restrained palette (1 primary, 1–2 accents, neutrals) used compositionally.
- Overlap, scale shifts, and layering create depth where it serves the story.
- Footer, nav, and forms receive the same design attention as heroes.

**Deduct for**: three-equal-cards rows, gradient-purple-hero defaults,
icon+title+paragraph modules repeated verbatim.

## 5. Mobile (0–10)

Premium means premium at 390px, not just at 1440px.

- Tap targets ≥44px; primary thumb-zone placement for key actions.
- Type scales fluidly (`clamp()`), no cramped or oversized text.
- Horizontal overflow eliminated at common widths (320/390/430px).
- Sticky elements don't consume more than ~20% of the viewport.
- Touch alternatives replace hover-dependent interactions.
- Images use proper `srcset`/`sizes`; hero loads fast on throttled 3G.
- Mobile nav feels native-quality: smooth open/close, focus handled, scroll locked.

## 6. Accessibility (0–10)

Premium includes everyone by definition.

- Contrast meets WCAG AA (4.5:1 body, 3:1 large) — including text over images/gradients.
- Focus states match the design language; never removed without a
  replacement.
- Semantic landmarks, logical heading order, skip link present.
- Interactive elements are real buttons/links with accessible names.
- Form fields have persistent labels (placeholder-as-label fails).
- Color is never the sole carrier of meaning.
- Screen-reader pass: announcements make sense out of context.

## 7. Performance (0–10)

Perceived speed is part of the aesthetic.

- LCP < 2.5s, INP < 200ms, CLS < 0.1 on mid-tier mobile hardware.
- Fonts preloaded/subsetted; `font-display` avoids invisible text.
- Hero media optimized (AVIF/WebP, correct dimensions, priority hint).
- Animations run on transform/opacity only; no layout-thrash jank.
- JS budget respected: hydration cost justified per feature.
- Skeletons/placeholders prevent layout shift and blank flashes.
- Test on throttled CPU + network, not a fast dev machine.

---

## Self-Check List

Before declaring the QA pass complete, answer honestly:

1. **Would this look generic / AI-templated?** If any section could be
   swapped into another site without edits, it's generic. Rework it
   until it could only belong to this brand.
2. **Does every animation have purpose?** Name what each animation
   communicates. If you can't name it, remove or redesign it.
3. **Would a professional designer approve this?** Evaluate against
   studio-grade work, not "better than average." If any section reads
   as developer-art instead of design-art, it isn't done.

Also verify: would you screenshot and share the site publicly? Does
polish hold from hero to footer, including error states?

## Verdict Format

```
Hierarchy:      x/10
Typography:     x/10
Motion:         x/10
Composition:    x/10
Mobile:         x/10
Accessibility:  x/10
Performance:    x/10
Total:          xx/70  → PASS / FAIL (blocking issues listed below)
```
