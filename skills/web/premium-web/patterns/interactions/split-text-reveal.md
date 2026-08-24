# Split-Text Reveal

Split a headline into per-character or per-word spans, then stagger them in as the
block scrolls into view — the classic premium hero effect. This pattern documents the
**split mechanics** specifically (SplitType vs. manual DOM splitting); see
[text-reveal.md](./text-reveal.md) for whole-block fade/slide reveals.

## When to use

- Hero headlines, section titles, short pull quotes.
- Anywhere typographic motion beats box motion; keep targets under ~120 characters.

## Approach 1 — SplitType + GSAP

```bash
npm install gsap split-type
```

```js
import { gsap } from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
import SplitType from "split-type";

gsap.registerPlugin(ScrollTrigger);

export function splitTextReveal(el) {
  // Reduced motion: skip the split entirely, leave the element untouched.
  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    return () => {};
  }

  const split = new SplitType(el, { types: "words,chars" });
  el.setAttribute("aria-label", el.textContent.trim());
  split.words.forEach((w) => w.setAttribute("aria-hidden", "true"));

  const tween = gsap.from(split.chars, {
    yPercent: 110,
    opacity: 0,
    stagger: 0.02,
    duration: 0.7,
    ease: "power4.out",
    scrollTrigger: { trigger: el, start: "top 80%", once: true },
  });

  // Once revealed, restore the original node so text stays selectable/copyable.
  tween.eventCallback("onComplete", () => split.revert());

  return () => {
    tween.scrollTrigger?.kill();
    tween.kill();
    split.revert();
  };
}
```

## Approach 2 — manual splitting, zero dependencies

```js
function manualSplit(el) {
  const text = el.textContent.trim();
  el.setAttribute("aria-label", text);
  el.innerHTML = "";

  for (const token of text.split(/(\s+)/)) {
    if (!token || /^\s+$/.test(token)) {
      el.appendChild(document.createTextNode(" "));
      continue;
    }
    const word = document.createElement("span");
    word.className = "st-word";
    word.setAttribute("aria-hidden", "true");
    for (const ch of token) {
      const char = document.createElement("span");
      char.className = "st-char";
      char.textContent = ch;
      word.appendChild(char);
    }
    el.appendChild(word);
  }
}
```

## Required CSS

```css
[data-split-reveal] { overflow: hidden; } /* clip chars rising from below */
.st-word { display: inline-block; white-space: nowrap; }
.st-char { display: inline-block; will-change: transform; }
```

`display: inline-block` is mandatory — transforms are ignored on plain inline spans.

## Usage

```html
<h1 data-split-reveal>Ship interfaces people feel.</h1>
<script type="module">
  document.querySelectorAll("[data-split-reveal]").forEach(splitTextReveal);
</script>
```

## Accessibility notes

- Screen readers skip the `aria-hidden` spans and announce the container's
  `aria-label`, keeping text readable and selectable despite the DOM splitting.
- Never split interactive elements (`a`, `button`) directly — split an inner
  `<span>` so hit targets remain intact.
- Wait for `document.fonts.ready` before splitting (late fonts shift metrics);
  on resize, revert and re-split, debounced.
