# Micro-interactions for Premium Websites

Micro-interactions separate a site that works from a site that feels
expensive: a button leaning toward the cursor, an image settling into place
on scroll. Done well they are invisible — users only notice their absence.
Everything below obeys three laws: respond to real input, communicate
something (affordance, hierarchy, state), finish fast and ease naturally.

## Magnetic Buttons

A magnetic button translates toward the cursor inside a proximity radius,
then springs back on exit. Track `mousemove`, compute the vector from
button center to pointer, and apply a fraction of that vector as a
transform:

```js
const magnets = [...document.querySelectorAll('[data-magnetic]')];
const STRENGTH = 0.35; // travel fraction — keep between 0.2 and 0.4
const RADIUS = 120;    // pull zone in px, roughly 1.5–2× the button size

window.addEventListener('mousemove', (e) => {
  requestAnimationFrame(() => {
    magnets.forEach((el) => {
      const r = el.getBoundingClientRect();
      const dx = e.clientX - (r.left + r.width / 2);
      const dy = e.clientY - (r.top + r.height / 2);
      el.style.transform = Math.hypot(dx, dy) < RADIUS
        ? `translate(${dx * STRENGTH}px, ${dy * STRENGTH}px)`
        : 'translate(0, 0)';
    });
  });
});
```

What separates a good magnet from a gimmick:

- Move a **fraction** of the distance, never the full vector — full tracking
  reads as broken, not magnetic.
- Spring back on exit with `cubic-bezier(0.22, 1, 0.36, 1)`; strip the
  transition while actively tracking.
- Move the inner label at higher strength than the shell for layer parallax;
  disable entirely under `(hover: none), (pointer: coarse)`.

## Custom Cursor Patterns

A custom cursor earns its place only by being more informative than the
arrow. Two patterns cover nearly every case:

**Dot + trailing ring.** The dot pins to the pointer; the ring follows with
a lerp — the lag *is* the fluidity.

```js
function loop() {
  ringX += (mouseX - ringX) * 0.15; // lerp factor 0.1–0.2
  ringY += (mouseY - ringY) * 0.15;
  ring.style.transform = `translate(${ringX}px, ${ringY}px)`;
  requestAnimationFrame(loop);
}

// Contextual morphing: mark targets, swap the ring state on hover
// <a href="/work/aurora" data-cursor="view">Aurora — case study</a>
link.addEventListener('mouseenter', () => {
  ring.dataset.state = link.dataset.cursor; // ring grows, shows "View"
});
```

- `cursor: none` demands a working replacement everywhere, including over
  inputs and text; hit testing stays native — the visual cursor decorates.
- Gate behind `(pointer: fine)`; restore the native cursor under
  `prefers-reduced-motion`.

## Hover States With Purpose

Every hover must answer: **what does the user learn by hovering?**

| Purpose    | Pattern                                          |
| ---------- | ------------------------------------------------ |
| Affordance | Underline grows from the left edge on links      |
| Preview    | Card lifts; "View project →" fades in            |
| Feedback   | Surface darkens ~10%, confirming pointer contact |
| Spotlight  | Siblings dim; the hovered item stays lit         |

If you cannot name the purpose, delete the effect — a bare `scale(1.05)`
teaches nothing and reads as template noise.

- Transition only `opacity` and `transform`, never `width` or margins.
- Set `transform-origin` deliberately; underlines read best growing leftward.
- Pair a small lift (`translateY(-2px)`) with a deepening shadow; hold hovers
  at 150–250ms — slower lags, faster twitches.

## Text and Image Reveal on Scroll

The reliable pattern: **IntersectionObserver toggles a class; CSS animates
it.** JavaScript detects; CSS moves — never mix the two jobs.

```css
.reveal {
  opacity: 0;
  transform: translateY(24px);
  transition: opacity 600ms cubic-bezier(0.22, 1, 0.36, 1),
    transform 600ms cubic-bezier(0.22, 1, 0.36, 1);
}

.reveal.is-visible {
  opacity: 1;
  transform: translateY(0);
}
```

```js
const io = new IntersectionObserver((entries) => {
  entries.forEach((entry) => {
    if (!entry.isIntersecting) return;
    entry.target.classList.add('is-visible');
    io.unobserve(entry.target); // fire once — never re-animate
  });
}, { threshold: 0.15, rootMargin: '0px 0px -10% 0px' });

document.querySelectorAll('.reveal').forEach((el) => io.observe(el));
```

- **Line mask:** wrap each line in an `overflow: hidden` span; the inner span
  slides up from `translateY(110%)`. Split lines at build time.
- **Curtain:** image scales `1.15 → 1` as a cover panel wipes away with
  `scaleX(1 → 0)`; `clip-path` works equally well.
- **Stagger:** `transition-delay: i * 60ms` per child so groups cascade.

Rules: threshold ~0.15 ("actually visible"), never `0`; keep hidden-state
styles behind a `.js` class on `<html>` so content survives without
JavaScript; cap the whole moment near 700ms including stagger.

## No Interaction for Its Own Sake

An effect added because it looked cool in a demo is a bug with motion
attached. Hold every candidate to one question:

> If we deleted this interaction, would the user lose information,
> orientation, or feedback?

- Magnetic buttons pass on a primary CTA; applied to every element they
  fail — nothing stands out anymore.
- Custom cursors pass on portfolios, where browsing *is* the product; they
  fail on dashboards, where precision matters.
- Reveals pass when they sequence content as the user reads; fading every
  paragraph makes pages feel slower than they are.

Decoration posing as interaction: parallax on body copy, tilt cards in data
tables, sound on hover, loaders that outlast the load. When in doubt, cut —
restraint, flawlessly executed, is the actual premium signal.
