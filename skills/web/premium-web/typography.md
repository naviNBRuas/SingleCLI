---
name: premium-web-typography
description: Type systems for premium sites — scale, display/body pairing, vertical rhythm, and shift-free font loading.
---

# Typography for Premium Websites

Typography is the first thing users perceive and the last thing they consciously notice. On premium surfaces the type system must feel intentional at every size: a clear scale, a disciplined pairing, even rhythm, and zero layout shift while fonts load.

## 1. Build a modular type scale

Pick one base size (16px) and one ratio; derive every other step from them. Common ratios:

| Ratio | Value | Feel |
| --- | --- | --- |
| Major second | 1.125 | Dense, editorial |
| Major third | 1.25 | Balanced default |
| Perfect fourth | 1.333 | Confident, marketing |
| Golden ratio | 1.618 | Dramatic, poster-like |

Rules that keep a scale premium rather than noisy:

- Ship **6–8 named steps**, never ad-hoc sizes (`--text-xs` … `--text-display`).
- Make display steps fluid instead of breakpoint-jumpy:

```css
--text-display: clamp(2.75rem, 1.2rem + 5vw, 5.5rem);
```

- Tighten tracking as size grows: `-0.01em` to `-0.03em` above 48px, `0` for body. Optical sizing (`opsz`) automates this where available.
- Use tabular numerals (`font-variant-numeric: tabular-nums`) wherever numbers align vertically.

## 2. Pair a display face with a body face

Two families, two jobs:

- **Display** — personality: headlines, pull quotes, hero numerals. Higher contrast, tighter spacing, often a serif or expressive grotesque.
- **Body** — invisible workhorse for paragraphs, UI, and forms. Neutral proportions, sturdy x-height, legible at 14px.

Proven free-license pairings: Fraunces + Inter, Playfair Display + Source Sans 3, Newsreader + Public Sans, Space Grotesk + IBM Plex Sans.

Pairing checklist:

1. Match x-heights so swapping faces doesn't force retuning sizes.
2. Contrast on at least one axis: serif/sans, warm/neutral, round/angular.
3. Cap families at two, plus an optional monospace for code and data.
4. Load at most four static weights per family — or use a variable font.
5. Never let the browser synthesize bold/italic; load the true styles you use.

## 3. Line-height and measure

Leading and line length carry the "expensive" feel more than the choice of font itself.

- Body measure: **45–75 characters**; ~66 is the classic target. Enforce with `max-width: 66ch`, not pixel guesses.
- Leading scales inversely with size:

| Style | Size | line-height |
| --- | --- | --- |
| Display | ≥ 48px | 1.05–1.15 |
| Headings | 24–47px | 1.15–1.3 |
| Lede/subheads | 18–23px | 1.35–1.45 |
| Body | 14–17px | 1.5–1.65 |
| Captions/legal | ≤ 13px | 1.4–1.5 |

- Build vertical rhythm from one spacing unit (4px or 8px); space stacks in multiples so baselines read as aligned without a strict baseline grid.
- Apply `text-wrap: balance` to headings and `text-wrap: pretty` to paragraphs to remove orphaned words.
- For narrow or justified columns enable `hyphens: auto` with a correct `lang`; otherwise keep ragged-right alignment.

## 4. Font loading without layout shift

Web fonts are render-blocking by default and the leading cause of CLS on text-heavy pages.

### Strategy

1. Self-host **WOFF2 only**; subset to the scripts you actually serve.
2. Preload only the one or two files needed for first paint:

```html
<link rel="preload" as="font" type="font/woff2"
      crossorigin href="/fonts/body-latin.woff2">
```

3. Choose `font-display` per role:
   - `swap` — brand/body text where a brief fallback flash is acceptable.
   - `optional` — hero/display type where shift is unacceptable (renders within ~100ms or stays fallback).
   - Avoid bare `auto`/`block`: invisible-text windows delay LCP and frustrate users.
4. Give the fallback metric-compatible overrides so swaps don't reflow the page (compute values with tools like Fontaine):

```css
@font-face {
  font-family: "Inter Fallback";
  src: local("Arial");
  size-adjust: 107%;
  ascent-override: 90%;
  descent-override: 25%;
  line-gap-override: 0%;
}
```

5. Prefer variable fonts: one file spans every weight and style, cutting requests and unlocking intermediate weights. Subset axes (`wght`, `opsz`, …) to what you use — an untrimmed variable family can outweigh two static cuts.

```css
@font-face {
  font-family: "Inter";
  src: url("/fonts/inter-var.woff2") format("woff2-variations");
  font-weight: 100 900;
  font-style: normal;
  font-display: swap;
}
```

### Guardrails

- Verify in Lighthouse/WebPageTest that no CLS is attributed to fonts; anything above ~0.02 means your fallback metrics are off.
- Cache immutably: hashed filenames plus `Cache-Control: max-age=31536000, immutable`.
- Delete unused weights and scripts; every unused face is paid for on each cold load.

## Quick checklist

- [ ] Fluid scale of 6–8 steps derived from one ratio
- [ ] Two families max: display with personality, body with stamina, matched x-heights
- [ ] Measure 45–75ch; leading 1.5–1.65 body, 1.05–1.3 display
- [ ] WOFF2-only, subsetted, critical files preloaded, cached immutable
- [ ] `font-display` chosen per role with a metric-compatible fallback
- [ ] Zero font-attributable CLS in lab and field data
