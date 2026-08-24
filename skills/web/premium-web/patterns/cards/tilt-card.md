# Tilt Card

A premium "3D tilt" interaction: the card rotates subtly toward the cursor,
giving it depth and a physical feel. Driven entirely by `transform`, so it
stays on the compositor thread and never triggers layout or paint.

**Use for:** hero cards, pricing tiers, portfolio tiles, feature highlights.
**Avoid for:** dense lists and data-heavy layouts — tilt everywhere is noise
and costs battery on low-end hardware.

## Core idea

1. Wrap the card in a **scene** element with `perspective`.
2. On `pointermove`, compute normalized cursor offsets (`-0.5..0.5`) from the
   card's center.
3. Map those offsets to `rotateX` / `rotateY` (max ±8deg is plenty).
4. Smoothly reset on `pointerleave`.

## Vanilla JS implementation sketch

```html
<div class="tilt-scene">
  <article class="tilt-card">…</article>
</div>
```

```css
.tilt-scene {
  perspective: 900px;
}

.tilt-card {
  transition: transform 160ms ease-out;
  transform-style: preserve-3d;
}

/* Optional inner layer that pops toward the viewer */
.tilt-card .tilt-pop {
  transform: translateZ(30px);
}
```

```js
const MAX_TILT = 8;

function initTilt(card) {
  let rect = null;
  let frame = 0;
  let rx = 0, ry = 0;

  const apply = () => {
    frame = 0;
    card.style.transform =
      `rotateX(${rx.toFixed(2)}deg) rotateY(${ry.toFixed(2)}deg)`;
  };

  card.addEventListener('pointerenter', () => {
    rect = card.getBoundingClientRect(); // cached once per gesture
  });

  card.addEventListener('pointermove', (e) => {
    if (!rect || e.pointerType === 'touch') return;

    const nx = (e.clientX - rect.left) / rect.width - 0.5;
    const ny = (e.clientY - rect.top) / rect.height - 0.5;
    ry = nx * MAX_TILT * 2;
    rx = -ny * MAX_TILT * 2;

    if (!frame) frame = requestAnimationFrame(apply);
  });

  card.addEventListener('pointerleave', () => {
    cancelAnimationFrame(frame);
    frame = 0;
    rect = null;
    card.style.transform = 'rotateX(0deg) rotateY(0deg)';
  });
}
```

Why these details matter:

- `getBoundingClientRect()` is called **once per enter**, never per move —
  re-measuring inside `pointermove` is the classic layout-thrash bug here.
- Style writes are batched into one `requestAnimationFrame` tick so bursts of
  mouse events collapse into a single write per frame.
- The transition is short (≤200ms); longer ones fight the pointer and lag.

## Framer Motion alternative

If the app already ships Framer Motion, springs give you smoothing for free:

```jsx
import { useRef } from 'react';
import {
  motion, useMotionValue, useSpring, useTransform,
} from 'framer-motion';

export function TiltCard({ children }) {
  const x = useMotionValue(0);
  const y = useMotionValue(0);

  const sx = useSpring(x, { stiffness: 250, damping: 25 });
  const sy = useSpring(y, { stiffness: 250, damping: 25 });

  const rotateX = useTransform(sy, [-0.5, 0.5], ['8deg', '-8deg']);
  const rotateY = useTransform(sx, [-0.5, 0.5], ['-8deg', '8deg']);
  const rectRef = useRef(null);

  return (
    <div style={{ perspective: 900 }}>
      <motion.div
        className="tilt-card"
        style={{ rotateX, rotateY, transformStyle: 'preserve-3d' }}
        onPointerEnter={(e) => {
          if (e.pointerType !== 'touch')
            rectRef.current = e.currentTarget.getBoundingClientRect();
        }}
        onPointerMove={(e) => {
          if (!rectRef.current) return;
          const r = rectRef.current;
          x.set((e.clientX - r.left) / r.width - 0.5);
          y.set((e.clientY - r.top) / r.height - 0.5);
        }}
        onPointerLeave={() => {
          rectRef.current = null;
          x.set(0); // springs animate the reset automatically
          y.set(0);
        }}
      >
        {children}
      </motion.div>
    </div>
  );
}
```

No manual rAF needed — motion values already update per-frame without extra
React renders.

## Touch devices: no tilt, just tap

Tilt presumes a hovering pointer. Touch has neither hover nor a cheap way to
track finger position before lift-off, so degrade instead of faking it:

- Bail out when `e.pointerType === 'touch'` (both versions above already do).
- Gate any hover-only styles behind `@media (hover: hover) and (pointer: fine)`.
- On touch, the card renders flat with a static elevated shadow; tapping
  activates it like any normal link or button. An `:active` press scale is a
  nice minimal substitute for the tilt.

Skip `DeviceOrientationEvent` gyroscope tilt unless it is a core brand moment:
iOS gates it behind a permission prompt and it burns battery.

## Performance notes

- **Transform only.** Never animate `top/left/width/height` or change
  `box-shadow` size per frame — both force layout + paint. Fake glow changes
  with a pre-rendered shadow layer whose `opacity` you fade instead.
- **Cache the rect.** One `getBoundingClientRect()` per gesture, not per event.
- **Batch writes.** Vanilla: single rAF loop. React: motion values, not state.
- `will-change: transform` promotes a GPU layer; apply it only while hovered,
  or trust the browser to promote during the animation. Dozens of always-on
  layers waste memory.
- Use `pointer` events rather than `mouse` events so one code path covers
  mouse, pen, and touch.
- Respect `prefers-reduced-motion`: render a flat card, keep the shadow.

## Accessibility

The tilt is purely decorative. Focus outlines, keyboard activation, and screen
reader semantics must remain untouched — never gate content or actions behind
the hover state itself.

## Checklist

- [ ] Scene wrapper sets `perspective`
- [ ] Max tilt ≤ 10deg, transition ≤ 200ms
- [ ] Rect cached on enter, writes rAF/motion-value batched
- [ ] Touch devices get a flat, tappable card
- [ ] `prefers-reduced-motion` disables rotation
- [ ] Keyboard focus and semantics unaffected
