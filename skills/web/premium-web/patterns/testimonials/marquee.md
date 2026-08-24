# Testimonial Marquee

Reusable infinite-scrolling marquee for testimonials or client logos.
Pure CSS animation — JavaScript is optional and only needed to clone the content
if your templating layer cannot render the list twice.

## How it works

1. A `.marquee` wrapper clips a `.marquee__track` that is wider than itself.
2. The track contains the item list **twice** (the second copy is `aria-hidden`).
3. The track animates `translateX(0 → -50%)`; because both halves are identical,
   the jump back to `0` is invisible — that's the seamless loop.
4. Hovering or keyboard-focusing inside pauses playback via `animation-play-state`.

## Markup

```html
<section class="marquee" aria-label="Customer testimonials">
  <div class="marquee__viewport">
    <ul class="marquee__track">
      <li class="marquee__card">
        <p>&ldquo;Best purchase we made this year.&rdquo;</p>
        <footer>&mdash; Dana K., CTO at ExampleCorp</footer>
      </li>
      <li class="marquee__card"><!-- more cards… --></li>
      <li class="marquee__card"><!-- 4&ndash;8 cards total --></li>
    </ul>
    <ul class="marquee__track" aria-hidden="true">
      <!-- byte-identical duplicate of the <li> set above -->
    </ul>
  </div>
</section>
```

Render the duplicate in your templating layer (`{% include "testimonial-list.html" %}`
twice), or fall back to one line of JS on load:

```js
viewport.append(...[...track.children].map(node => node.cloneNode(true)));
```

## Styles

```css
.marquee {
  overflow: hidden;
  /* fade the edges so items slide in/out instead of popping */
  mask-image: linear-gradient(to right, transparent, black 8%, black 92%, transparent);
}

.marquee__viewport {
  display: flex;
}

.marquee__track {
  display: flex;
  gap: 2rem;
  flex-shrink: 0;
  min-width: max-content;
  padding-inline-end: 2rem; /* makes the seam gap match the item gap exactly */
  animation: marquee var(--marquee-duration, 40s) linear infinite;
}

.marquee:hover .marquee__track,
.marquee:focus-within .marquee__track {
  animation-play-state: paused;
}

@keyframes marquee {
  from { transform: translateX(0); }
  to   { transform: translateX(calc(-50% - 1rem)); } /* half the seam padding */
}
```

Timing rule of thumb: `--marquee-duration ≈ unique-items × 6s`. Slow enough to read.

## Reduced motion

Under `prefers-reduced-motion: reduce`, stop the animation and turn the strip into
a static wrapped grid instead of content scrolling past unreadably:

```css
@media (prefers-reduced-motion: reduce) {
  .marquee { overflow: visible; mask-image: none; }
  .marquee__viewport { flex-wrap: wrap; }
  .marquee__track { animation: none; min-width: 0; }
}
```

Since the second copy carries `aria-hidden="true"`, assistive tech reads each
testimonial exactly once in either layout mode.

## Usage notes

- Children are agnostic: testimonial cards, `<img>` logos, plain spans all work.
- For logos use real images with alt text (`<img src="acme.svg" alt="Acme Corp">`)
  in the *first* copy; the duplicate is hidden from AT anyway.
- Multiple marquees on one page: scope `--marquee-duration` per instance.
- Ensure enough items exist to exceed the viewport width; too few causes a gap
  before the loop wraps — add items or shorten the viewport.

## Performance notes

- Only `transform` animates, which stays on the compositor thread: no layout or
  repaint per frame, smooth even on mid-range phones.
- Do not animate `width`, `left`, or `margin` for the same effect — those trigger
  layout every frame.
- Skip `will-change` unless profiling shows jank; a single marquee promotes its
  own layer naturally during the animation.
- Bound the DOM: 4–10 unique items per copy is plenty. Duplication doubles nodes,
  which is trivial for dozens of cards but wasteful in the hundreds.
- Pause-on-hover doubles as a battery saver while users actually read a quote.
- If you compute duration from content width in JS, measure once on load and on
  resize (debounced) — never inside a frame loop.
