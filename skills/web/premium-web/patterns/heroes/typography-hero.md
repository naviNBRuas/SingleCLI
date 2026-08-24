# Typography Hero

An oversized display-type hero where **the words themselves are the visual**.
No photography, illustration, or gradient mesh required — scale, weight,
tracking, and motion do all of the work. Best-in-class examples: studio
landing pages, portfolio intros, product launch teasers.

## When to reach for this

- Brand or studio site with strong typographic identity
- No (or weak) imagery assets available
- You want instant art direction: type IS the brand
- Editorial/manifesto tone ("We build calm software.")

Avoid when: the page must communicate a concrete product screenshot on first
paint, or when copy is long — this pattern dies past ~12 words.

## Anatomy

```
┌─────────────────────────────────────────┐
│  eyebrow (small caps, tracked out)      │
│                                         │
│  MEGA WORD ONE                          │  ← clamp() display line
│  mega word two                          │  ← contrast weight/style
│                                         │
│  short supporting sentence ────────     │
│  [ CTA ]   [ ghost CTA ]                │
└─────────────────────────────────────────┘
```

## Implementation sketch

```html
<section class="type-hero">
  <p class="type-hero__eyebrow">Studio — Est. 2019</p>
  <h1 class="type-hero__title">
    <span class="reveal-line" data-split>We design</span>
    <span class="reveal-line reveal-line--accent" data-split>quiet interfaces</span>
  </h1>
  <p class="type-hero__sub">Software that stays out of your way.</p>
</section>
```

```css
.type-hero {
  min-height: 92svh;
  display: grid;
  align-content: center;
  gap: clamp(1rem, 3vw, 2.5rem);
  padding-inline: clamp(1.25rem, 6vw, 6rem);
}

.type-hero__eyebrow {
  font-size: 0.75rem;
  letter-spacing: 0.35em;
  text-transform: uppercase;
}

.type-hero__title {
  /* Fluid display scale: 3rem → 11vw, capped at 9.5rem */
  font-size: clamp(3rem, 4vw + 8vmin, 9.5rem);
  line-height: 0.95;          /* tight leading = poster feel */
  letter-spacing: -0.03em;    /* negative tracking at large sizes */
  text-wrap: balance;
}

.reveal-line {
  display: block;
  overflow: hidden;           /* clip mask for the rise animation */
}

.reveal-line--accent {
  font-style: italic;
  font-weight: 200;           /* weight contrast against the bold line */
}
```

### Split-word / char scroll-reveal

Break each `.reveal-line` into word spans, then char spans, then animate
`translateY(110%) → 0` with per-char stagger. Keep it dependency-light:

```js
function splitChars(el) {
  const frag = document.createDocumentFragment();
  el.textContent.split(/(\s+)/).forEach((word) => {
    if (/^\s+$/.test(word)) return frag.append(word);
    const wrap = document.createElement("span");
    wrap.className = "word";
    [...word].forEach((ch) => {
      const inner = document.createElement("span");
      inner.className = "char";
      inner.textContent = ch;
      wrap.append(inner);
    });
    frag.append(wrap);
  });
  el.replaceChildren(frag);
}

document.querySelectorAll("[data-split]").forEach(splitChars);

const io = new IntersectionObserver(
  ([entry]) => entry.isIntersecting && entry.target.classList.add("is-in"),
  { threshold: 0.4 }
);
io.observe(document.querySelector(".type-hero__title"));
```

```css
.char {
  display: inline-block;
  translate: 0 110%;
  rotate: 0.06turn;                 /* slight tumble as chars rise */
  transition:
    translate 0.7s cubic-bezier(0.22, 1, 0.36, 1),
    rotate 0.7s cubic-bezier(0.22, 1, 0.36, 1);
  transition-delay: calc(var(--i, 0) * 28ms);
}

.is-in .char { translate: 0 0; rotate: 0turn; }
```

Set `--i` per char inside `splitChars` (`inner.style.setProperty("--i", i)`).

## Responsive type scaling rules

1. One `clamp()` per display element: `clamp(min, fluid, max)`.
2. Fluid term mixes viewport units with a fixed floor so mid-sizes feel
   intentional: `clamp(3rem, 4vw + 8vmin, 9.5rem)`.
3. Never let a line wrap mid-word at mobile widths — reduce the word count
   instead of the minimum size below ~2.5rem.
4. Tighten leading and tracking *as size grows*; keep body copy untouched.

## Reduced-motion fallback

```css
@media (prefers-reduced-motion: reduce) {
  .char {
    translate: none;
    rotate: none;
    transition: none;
  }
  /* Or skip splitting entirely: gate splitChars() on matchMedia */
}
```

JS-side, honor it before touching the DOM:

```js
const prefersReduced =
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;
if (!prefersReduced) document.querySelectorAll("[data-split]").forEach(splitChars);
else document.querySelectorAll(".type-hero__title").forEach((t) => t.classList.add("is-in"));
```

Unsplit text is fully visible by default — no-JS users see static type.

## Checklist

- [ ] Contrast ≥ 4.5:1 for accent line against background
- [ ] Works with JS disabled (static, visible headline)
- [ ] `prefers-reduced-motion` honored (CSS *and* JS gates)
- [ ] No horizontal overflow at 320px
- [ ] Headline reads as plain text to screen readers after split (aria-hidden
      the split spans only if you duplicate accessible text — prefer keeping
      real characters in DOM)
