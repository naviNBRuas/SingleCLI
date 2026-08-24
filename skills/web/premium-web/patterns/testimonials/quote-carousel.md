# Quote Carousel

A testimonial carousel that rotates customer quotes with fade or slide
transitions, auto-advances on a timer, pauses on user interaction, and
supports dot + arrow navigation. Built for marketing pages where social
proof needs to earn attention without stealing focus.

## Behavior contract

- One quote is visible at a time; transitions are `opacity` (fade) or
  `transform: translateX` (slide) only — never `height`/`width`.
- Auto-advance every 6–8s. Any pointerenter, focusin, or touchstart on the
  region stops the timer; pointerleave/focusout restarts it after a grace
  delay. The timer also resets after manual navigation.
- Dots reflect and control the active index; prev/next arrows step through,
  wrapping at the ends.
- With `prefers-reduced-motion: reduce`: no auto-advance, instant switches
  (no transition), dots/arrows still fully functional.
- Announce changes politely via an `aria-live="polite"` status region —
  never make the whole track live, or screen readers will re-read every
  quote on each transition.

## Implementation sketch

```html
<section class="quote-carousel" data-quote-carousel aria-roledescription="carousel" aria-label="Customer testimonials">
  <div class="qc-track">
    <figure class="qc-slide is-active" role="group" aria-roledescription="slide" aria-label="1 of 4">
      <blockquote>“This team shipped in three weeks what our last vendor couldn't in six months.”</blockquote>
      <figcaption>Dana R. — Head of Ops, Northwind</figcaption>
    </figure>
    <figure class="qc-slide" hidden role="group" aria-roledescription="slide" aria-label="2 of 4">…</figure>
    <!-- remaining slides -->
  </div>

  <div class="qc-controls">
    <button type="button" class="qc-prev" aria-label="Previous testimonial">‹</button>
    <div class="qc-dots" role="tablist" aria-label="Choose testimonial"></div>
    <button type="button" class="qc-next" aria-label="Next testimonial">›</button>
  </div>

  <p class="qc-status visually-hidden" role="status"></p>
</section>
```

```js
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)');
const root = document.querySelector('[data-quote-carousel]');
const slides = [...root.querySelectorAll('.qc-slide')];
const status = root.querySelector('.qc-status');
const dots = slides.map((_, i) => {
  const b = document.createElement('button');
  b.type = 'button';
  b.setAttribute('role', 'tab');
  b.setAttribute('aria-label', `Testimonial ${i + 1}`);
  b.addEventListener('click', () => goTo(i));
  return b;
});
root.querySelector('.qc-dots').append(...dots);

let index = 0;
let timer = null;

function render() {
  slides.forEach((slide, i) => {
    slide.classList.toggle('is-active', i === index);
    slide.toggleAttribute('hidden', i !== index && !reduceMotion.matches ? false : i !== index);
    // keep non-active slides in layout for CSS transitions if animating;
    // toggle `hidden` only when motion is reduced (instant switch)
  });
  dots.forEach((d, i) => d.setAttribute('aria-selected', String(i === index)));
  status.textContent = `Testimonial ${index + 1} of ${slides.length}`;
}

function goTo(next) {
  index = (next + slides.length) % slides.length;
  render();
  restart(); // manual nav resets the auto-advance clock
}

function stop() { clearInterval(timer); timer = null; }
function start() {
  if (!timer && !reduceMotion.matches) {
    timer = setInterval(() => goTo(index + 1), 7000);
  }
}
function restart() { stop(); start(); }

root.querySelector('.qc-prev').addEventListener('click', () => goTo(index - 1));
root.querySelector('.qc-next').addEventListener('click', () => goTo(index + 1));

for (const evt of ['pointerenter', 'focusin', 'touchstart']) {
  root.addEventListener(evt, stop, { passive: true });
}
root.addEventListener('pointerleave', restart);
root.addEventListener('focusout', (e) => {
  if (!root.contains(e.relatedTarget)) restart();
});

reduceMotion.addEventListener?.('change', () => { stop(); render(); });

// keyboard support on the region itself
root.addEventListener('keydown', (e) => {
  if (e.key === 'ArrowLeft') { e.preventDefault(); goTo(index - 1); }
  if (e.key === 'ArrowRight') { e.preventDefault(); goTo(index + 1); }
});

render();
start();
```

```css
.qc-slide { opacity: 0; transition: opacity 400ms ease; position: absolute; inset: 0; }
.qc-slide.is-active { opacity: 1; position: relative; }
@media (prefers-reduced-motion: reduce) {
  .qc-slide { transition: none; }
}
.visually-hidden { position: absolute; width: 1px; height: 1px; clip-path: inset(50%); overflow: hidden; white-space: nowrap; }
```

## Usage notes

- Keep quotes short (≤2 sentences) and attribute them with name + role +
  company; unattributed praise reads as fake.
- 3–5 slides maximum. Beyond that, engagement drops and maintenance cost
  rises faster than value.
- Place controls below the quote, aligned center, with ≥44px hit targets.
- Never autoplay more than one carousel per page, and never nest carousels.
- If quotes differ greatly in length, fix the container height to the
  tallest quote (or reserve space) to avoid layout shift during fades.

## Accessibility checklist

- Region has `aria-roledescription="carousel"` and an accessible label.
- Each slide is `role="group"` with `aria-roledescription="slide"` and an
  `aria-label` like "1 of 4".
- Status paragraph with `role="status"` announces index changes politely.
- Arrow keys navigate when focus is inside the carousel; buttons are
  reachable by Tab and operable by Enter/Space.
- Pause-on-hover must *not* be the only pause mechanism — keyboard users
  get pause via focusin, and touch users via tap.
