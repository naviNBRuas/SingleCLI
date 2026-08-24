# Scroll Progress Indicator

A fixed circular indicator that fills as the reader scrolls through an article.
Pinned to a corner of the viewport, it gives a constant sense of position on
long-form pages — blog posts, docs, essays. A top-edge linear bar is the same
pattern with different geometry.

## Behavior

- Empty ring at `scrollTop === 0`, full ring at maximum scroll; fills
  clockwise from 12 o'clock.
- Clicking it returns to the top (common affordance on article sites).
- Hidden entirely when there is nothing to scroll (short pages).

## Implementation sketch

Markup — one instance per page:

    <button class="scroll-progress" type="button"
            role="progressbar" aria-label="Article reading progress"
            aria-valuemin="0" aria-valuemax="100" aria-valuenow="0">
      <svg viewBox="0 0 48 48" aria-hidden="true">
        <circle class="track" cx="24" cy="24" r="20"/>
        <circle class="fill" cx="24" cy="24" r="20"/>
      </svg>
      <span class="percent">0%</span>
    </button>

CSS — circumference is `2πr ≈ 125.6`; the dash offset does the filling:

    .scroll-progress { position: fixed; right: 1.5rem; bottom: 1.5rem;
                       width: 3rem; height: 3rem; border: none;
                       background: transparent; cursor: pointer; padding: 0; }
    .track { fill: none; stroke: var(--border-subtle); stroke-width: 3; }
    .fill  { fill: none; stroke: var(--accent); stroke-width: 3; stroke-linecap: round;
             stroke-dasharray: 125.6; stroke-dashoffset: 125.6;
             transform: rotate(-90deg); transform-origin: center; }

JS — read scroll position at most once per animation frame:

    const btn   = document.querySelector(".scroll-progress");
    const fill  = btn.querySelector(".fill");
    const label = btn.querySelector(".percent");
    const CIRC  = 2 * Math.PI * 20;
    let ticking = false;

    function update() {
      const max = document.documentElement.scrollHeight - innerHeight;
      const pct = max > 0 ? Math.min(100, Math.round(scrollY / max * 100)) : 0;
      fill.style.strokeDashoffset = CIRC * (1 - pct / 100);
      btn.setAttribute("aria-valuenow", String(pct));
      label.textContent = pct + "%";
      btn.style.visibility = max > 0 ? "visible" : "hidden";
      ticking = false;
    }

    addEventListener("scroll", () => {
      if (!ticking) { requestAnimationFrame(update); ticking = true; }
    }, { passive: true });

    btn.addEventListener("click", () => scrollTo({ top: 0, behavior: "smooth" }));

## Usage instructions

1. Drop the markup before `</body>`; include CSS and JS once per layout, and
   call `update()` once on load so the initial state is correct.
2. Recompute `CIRC` if you change the circle radius.
3. To scope progress to an article element rather than the whole page
   (infinite-scroll feeds), use that element's bounding rect instead.

## Accessibility

- `role="progressbar"` with `aria-valuemin`, `aria-valuemax`, and a live
  `aria-valuenow` lets screen readers announce position on demand. Do **not**
  add `aria-live="polite"` — announcing every percent during scroll is noise;
  users query the control explicitly.
- Keep the visible `%` text synced with `aria-valuenow`, and give the button a
  visible focus style (it inherits native keyboard behavior).
- Respect `prefers-reduced-motion`: skip the smooth-scroll behavior and
  animate `stroke-dashoffset` with a transition of ≤150ms or none at all.

## Performance notes

- The `passive: true` scroll listener plus the rAF gate means at most one
  style write per frame — no jank on mobile.
- Cache `scrollHeight - innerHeight` and recompute only on `resize`; don't
  call `getBoundingClientRect()` in the handler more than needed.
- Prefer `IntersectionObserver` when progress is section-based: observe each
  heading block and derive percentage from entries, so scroll events do no
  main-thread work until visibility actually changes.
