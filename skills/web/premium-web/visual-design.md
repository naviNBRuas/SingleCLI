# Visual Design Fundamentals for Premium Websites

Principles for making a site look deliberately crafted rather than assembled.
Every rule here is a default, not a law — but deviate with a reason you can state.

## 1. Visual hierarchy

Hierarchy means the eye lands where *you* decide it lands, in the order *you*
choose. Establish exactly one focal point per viewport: on a landing hero it's
the headline + primary CTA; on a pricing card the recommended tier's price; on
a dashboard the number the user came to check. Everything else must be
visually quieter than the focal point.

Use these levers, roughly in order of strength:

1. **Size** — the dominant element should be 2–3x the next rank linearly, not
   10–20% bigger: a `clamp(2.5rem, 6vw, 4rem)` headline over `1rem` body reads
   as intentional; 36px over 28px reads as a mistake.
2. **Weight** — pair one heavy weight (600–700) with regular (400). Avoid
   three-plus weights per screen, and medium-vs-regular pairs — too subtle
   to register.
3. **Color saturation / darkness** — full-strength brand color or near-black
   for primary; muted grays (see §4) for secondary. Never make two elements
   "equally important but different colors".
4. **Space** — the most premium lever. Isolating an element with generous
   whitespace promotes it more reliably than enlarging it does.
5. **Position** — top-left entry (top-center for symmetric layouts) gets first
   fixation. Put what you want seen *first* there.

Squint test: blurred, you should still name the focal point and primary action;
if everything blurs into one gray mass, your size/weight steps are too small.

## 2. Composition

**One axis of alignment per section.** Pick left-aligned or centered text for
a section and commit; mixing them inside one block looks unresolved unless the
centering is doing clear work (e.g., a hero).

**Align to a grid, not to other elements.** Define a content column (`min(72ch,
90vw)` for reading content; 1140–1280px max-width for marketing pages) and snap
every section's edges to it. Elements aligned to *each other* but not the grid
create drift across sections.

**Asymmetry needs counterweights.** An offset image or overlapping card is
premium only when something else balances its visual mass diagonally opposite;
a floating element with nothing anchoring it reads as broken, not dynamic.

**Group by proximity, not boxes.** Before reaching for borders/cards, tighten
space within a group and expand space between groups (see §3). Cards are for
interactive or clearly bounded objects, not for wrapping arbitrary paragraphs.

**Direct the scan path.** Western users scan F/Z patterns. Place the primary
CTA at the end of the natural scan: headline → supporting line → button, each
step pulling downward. Repeating a shape at irregular intervals creates rhythm;
perfect repetition creates wallpaper.

## 3. Spacing rhythm: an 8pt-style scale

Use a spacing scale where every value is a multiple of 4 (base), with major steps at multiples of 8:

```
--space-3xs: 4px    /* icon-to-label, chip padding */
--space-2xs: 8px    /* intra-group gaps, badge padding */
--space-xs: 12px    /* form field internal padding */
--space-sm: 16px    /* paragraph spacing, small card padding */
--space-md: 24px    /* card padding, list item separation */
--space-lg: 32px    /* between grouped blocks */
--space-xl: 48px    /* between subsections */
--space-2xl: 64px   /* section padding-top/bottom (desktop) */
--space-3xl: 96px   /* major section breaks */
--space-4xl: 128px+ /* hero breathing room, page-level rhythm */
```

Why multiples of 4/8 instead of arbitrary values:

- **Sub-pixel rendering.** Odd values blur borders and misalign baselines on
  1dpr screens; even values land on whole device pixels at 1x and 2x.
- **Consistent optical math.** When every gap is from the scale, nested
  paddings sum back onto it (`24 + 24 = 48`); arbitrary values (`18 + 22`)
  don't, and misalignment compounds across components.
- **Fewer decisions.** Any value not on the scale is a bug or a documented
  exception — inconsistency becomes visible instead of invisible.

Rules of application:

- **Related-in, unrelated-out.** Space *within* a group ≤ space *between*
  groups, always; fix mispairings before tweaking pixels.
- **Headline-to-body gap ≈ 0.5–0.75× the headline's line-height** — headlines
  describe what follows, so they need less space below than above.
- **Section vertical rhythm ≥ 64px desktop, ≥ 48px mobile.** Premium sites
  breathe; tighter than that, no typography will save the page from feeling
  cramped.
- **Never hand-tune off-scale values.** If 20px seems needed, the real problem
  is usually font-size or line-height, not the gap.

## 4. Contrast

Contrast operates on multiple axes simultaneously: luminance, hue, size, and
weight. Hierarchy comes from *varying* them together.

**Text luminance tiers (light theme):**

- Primary text: `#111827`–`#1f2937`. Never pure `#000` — it vibrates on white.
- Secondary text: `#4b5563`–`#6b7280`; still passes AA (≥4.5:1) on white.
- Tertiary/meta text: `#9ca3af`, only for timestamps, captions, disabled
  states, and ≥14px so it stays legible. Never lighter than this for readable
  content — if text feels too prominent, reduce *size* or *weight* instead.

**Dark themes:** invert the logic — primary text `#e5e7eb`–`#f3f4f6`, not pure
white (pure white on near-black glares); backgrounds layered `#0b0f14` →
`#111827` → `#1f2937` as surfaces rise toward the viewer.

**Contrast ratios:** body text ≥ 7:1 (AAA) is the premium bar; UI labels ≥ 4.5:1;
large display text ≥ 3:1. Check *every* gray-on-color combination, especially
placeholders and button hover states.

**Size contrast is cheaper than color contrast.** A 64px light-weight headline
in the *same* ink as 16px body text still dominates completely. Reserve color
shifts for interactive/stateful elements (links, badges, alerts) so color stays
meaningful.

## 5. Depth and layering

Depth tells users what sits above what — build a strict elevation ladder and reuse it everywhere:

| Elevation | Use | Shadow recipe |
|---|---|---|
| 0 | Page background | none |
| 1 | Cards, sticky headers | `0 1px 2px rgb(0 0 0 / 0.06), 0 1px 3px rgb(0 0 0 / 0.08)` |
| 2 | Dropdowns, popovers, hover-lifted cards | `0 4px 8px -2px rgb(0 0 0 / 0.08), 0 2px 4px -2px rgb(0 0 0 / 0.06)` |
| 3 | Modals, drawers | `0 20px 25px -5px rgb(0 0 0 / 0.12), 0 8px 10px -6px rgb(0 0 0 / 0.10)` |
| 4 | Toasts, commands palette | `0 25px 50px -12px rgb(0 0 0 / 0.25)` |

Shadow rules that separate crafted from default:

- **Two-layer shadows.** One tight (small blur, low offset) plus one soft
  (large blur, larger offset) reads as realistic ambient + directional light.
  Single shadows read as clip-art.
- **Negative spread / negative Y offsets.** Shadows slightly narrower than the
  element (`spread: -2px`) hug it instead of haloing it.
- **Low opacity, always.** Max ~0.25 alpha even for modals; if a shadow needs
  more to be visible, the element probably needs a border instead.
- **Pair elevation with a hairline border** (`1px solid rgb(0 0 0 / 0.06)`
  light, `rgb(255 255 255 / 0.08)` dark). Border defines the edge, shadow sells
  the lift — together they look expensive; either alone looks thin.
- **Elevation implies motion rules.** Interaction-born surfaces (dropdowns,
  tooltips) sit higher than resting ones. Hovering a card may raise it one
  level (+150ms ease-out), never three.

**Gradients for depth, not decoration:**

- Subtle top-lighting on large surfaces: `linear-gradient(180deg, rgb(255 255 255 / 0.04), transparent 30%)` gives panels a lit-from-above feel without being identifiable as "a gradient".
- Radial glow behind a focal element: very low alpha (≤0.15), radius ≥ element size ×1.5.
- Avoid multi-hue rainbow gradients except as an explicit brand moment (max one per page). Two adjacent hues from the same ramp, ≤30° hue difference, always.
- Dark themes earn the most from gradients: surface-to-surface transitions (`#0b0f14` → `#111827`) replace borders for defining regions.

## 6. Visual density control

Density is information per unit area. Both extremes fail: too dense reads
cluttered and untrustworthy; too sparse reads empty and unfinished. Target
*controlled* density — dense where users work, airy where users decide.

**Diagnose which failure you have:**

- *Feels empty:* >1.5 viewports of one unbroken text column, orphaned
  headlines, sections whose content fills <50% of their declared height.
- *Feels cluttered:* >~3 font sizes + 2 weights per section, >2 competing
  accent colors, cards inside cards, borders AND shadows AND fills all marking
  the same boundary, line lengths >80ch.

**Fixing emptiness (without inventing filler):**

1. Narrow text-heavy sections to `68–72ch`; narrower columns read composed, not vacant.
2. Merge thin sections — two 300px sections beat one 700px section with a desert mid-page.
3. Pull one element out of the column rhythm (stat band, quote, product shot) to occupy the void with intent.
4. Increase type-scale contrast instead of whitespace: bigger headline, tighter leading.

**Fixing clutter:**

1. Remove boundaries before shrinking content: drop card borders/fills where spacing alone can group (§2).
2. Cut decoration tiers: choose border XOR shadow XOR fill per boundary — never all three.
3. Demote metadata: timestamps, IDs, counts move to tertiary color + smaller size, on hover where possible.
4. Enforce one accent color per viewport; every additional accent divides attention geometrically.
5. Free tabular data: remove vertical rules, keep horizontal hairlines (`1px rgb(0 0 0 / 0.06)`), right-align numbers.

**Calibration targets:** body line-height 1.5–1.7; paragraph measure 60–75
characters; list item padding ≥ 12px vertical; dashboard tiles ≥ 16px internal
padding; marketing sections ≥ 96px vertical padding desktop / 64px mobile. If a
section still feels off, count distinct sizes + weights + colors — above ~7, cut.

## Checklist before shipping a page

- [ ] Squint test: focal point and primary action identifiable when blurred
- [ ] All spacing values come from the scale; no odd pixel values
- [ ] One alignment axis per section; edges snapped to the content grid
- [ ] Text contrast meets AA minimums in both themes
- [ ] Shadows follow the elevation table; no single-layer or high-alpha shadows
- [ ] No element uses border + shadow + fill to mark the same boundary
- [ ] One accent color per viewport; gradients ≤2 hues, low alpha
