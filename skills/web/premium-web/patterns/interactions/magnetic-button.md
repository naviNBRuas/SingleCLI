# Magnetic Button

A button that gravitates toward the cursor: the closer the pointer gets, the
further it translates toward it; on pointer leave it springs back to rest.
Purely decorative motion — keyboard users just see a normal focusable button.

## Implementation

```js
// magnetic-button.js
const STRENGTH = 0.35;   // 0 = off, 1 = button center follows cursor exactly
const MAX_OFFSET = 24;   // px clamp so the button can't fly across the page

function initMagneticButton(el) {
  // Skip entirely on coarse pointers (touch) and reduced-motion users.
  if (!window.matchMedia('(pointer: fine)').matches) return;
  if (window.matchMedia('(prefers-reduced-motion: reduce)').matches) return;

  let raf = null;

  const onMove = (e) => {
    const rect = el.getBoundingClientRect();
    const dx = e.clientX - (rect.left + rect.width / 2);
    const dy = e.clientY - (rect.top + rect.height / 2);

    if (raf) return;
    raf = requestAnimationFrame(() => {
      raf = null;
      const x = Math.max(-MAX_OFFSET, Math.min(MAX_OFFSET, dx * STRENGTH));
      const y = Math.max(-MAX_OFFSET, Math.min(MAX_OFFSET, dy * STRENGTH));
      el.style.transform = `translate(${x}px, ${y}px)`;
    });
  };

  const onLeave = () => {
    if (raf) { cancelAnimationFrame(raf); raf = null; }
    el.style.transform = 'translate(0px, 0px)';
    el.style.transition = 'transform 420ms cubic-bezier(0.34, 1.56, 0.64, 1)';
    el.addEventListener('transitionend', () => { el.style.transition = ''; }, { once: true });
  };

  el.addEventListener('pointermove', onMove);
  el.addEventListener('pointerleave', onLeave);

  return () => {
    el.removeEventListener('pointermove', onMove);
    el.removeEventListener('pointerleave', onLeave);
  };
}
```

The spring-back uses a single overshooting cubic-bezier (`y > 1` on the second
control point) instead of a JS physics loop — one transition, GPU-composited,
no per-frame bookkeeping. If the project already ships Motion One or GSAP,
swap the manual transform writes for their spring utilities and drop the
manual transition handling.

## Usage

```html
<button class="btn" data-magnetic>Get started</button>
```

```js
document.querySelectorAll('[data-magnetic]').forEach(initMagneticButton);
```

```css
.btn {
  will-change: transform;   /* promote once, not per-frame */
  transition: transform 120ms linear; /* subtle trailing while following */
}
.btn:focus-visible { outline: 2px solid currentColor; outline-offset: 4px; }
```

Tuning:

- `STRENGTH` between `0.2`–`0.4` feels premium; above `0.5` feels broken.
- Apply to CTAs only. Magnetic nav links and cards read as gimmicky.
- Keep the visual translation small relative to layout — never translate a
  form submit button inside a tight grid cell where it can overlap siblings.

## Touch-device fallback

`matchMedia('(pointer: fine)')` returns false on phones/tablets, so listeners
are never attached and the button behaves as a plain button. If the app must
survive docking/undocking a tablet mid-session, re-check via the media
query's `change` event; otherwise one check at init is sufficient.

## Accessibility

- The transform is cosmetic only: `tabindex`, role, labels, and click handlers
  stay untouched, so screen readers announce a normal button.
- Keyboard focus (`:focus-visible`) triggers the standard outline — no motion
  is tied to focus, deliberately.
- `prefers-reduced-motion: reduce` disables the effect at init; the safest
  reduced-motion behavior is none at all.
- Do not add hover scaling to this pattern — combined translate + scale makes
  the target move while the user aims at it.

## Performance notes

- One throttled `getBoundingClientRect()` per `pointermove` via
  `requestAnimationFrame`; read happens before the write in the same frame,
  avoiding layout thrash.
- Only `transform` is animated — compositor-only, no layout/paint cost.
- `will-change: transform` is set in CSS up front; don't toggle it from JS on
  every enter/leave (forces layer re-promotion churn).
- Listeners are passive by default (no `preventDefault`); the returned cleanup
  removes them on unmount.
