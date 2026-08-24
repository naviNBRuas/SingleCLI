# Comparison Slider (Before / After)

A draggable vertical divider that reveals one image over another. Typical uses:
photo retouching reveals, redesign comparisons, product variant switches.

## Structure

```html
<figure class="cmp" style="--pos: 50%">
  <img src="after.jpg" alt="After state">
  <div class="cmp__clip">
    <img src="before.jpg" alt="Before state">
  </div>
  <div class="cmp__handle" role="slider" tabindex="0"
       aria-label="Comparison position"
       aria-valuemin="0" aria-valuemax="100" aria-valuenow="50"
       aria-valuetext="50 percent before visible"></div>
</figure>
```

## Styles

```css
.cmp { position: relative; overflow: hidden; touch-action: none; }
.cmp img { display: block; width: 100%; height: auto; user-select: none; }
.cmp__clip {
  position: absolute; inset: 0;
  clip-path: inset(0 calc(100% - var(--pos)) 0 0);
}
.cmp__clip img { height: 100%; object-fit: cover; }
.cmp__handle {
  position: absolute; top: 0; bottom: 0; left: var(--pos);
  width: 3px; margin-left: -1.5px; background: #fff; cursor: ew-resize;
}
.cmp__handle::after {
  /* grabber knob centered on the divider */
  content: ""; position: absolute; top: 50%; left: 50%;
  translate: -50% -50%; width: 40px; height: 40px; border-radius: 50%;
  background: #fff; box-shadow: 0 1px 6px rgb(0 0 0 / 0.4);
}
```

Everything derives from `--pos` (a percentage) set inline or by JS.

## Behavior

```js
function initComparison(root) {
  const handle = root.querySelector('.cmp__handle');
  const pos = v => {
    v = Math.min(100, Math.max(0, v));
    root.style.setProperty('--pos', `${v}%`);
    handle.setAttribute('aria-valuenow', String(Math.round(v)));
    handle.setAttribute('aria-valuetext', `${Math.round(v)} percent before visible`);
  };
  const fromPointer = e => {
    const r = root.getBoundingClientRect();
    pos(((e.clientX - r.left) / r.width) * 100);
  };
  let dragging = false;
  root.addEventListener('pointerdown', e => {
    dragging = true;
    root.setPointerCapture(e.pointerId);
    fromPointer(e);
  });
  root.addEventListener('pointermove', e => { if (dragging) fromPointer(e); });
  root.addEventListener('pointerup', () => { dragging = false; });
  root.addEventListener('pointercancel', () => { dragging = false; });
  handle.addEventListener('keydown', e => {
    const cur = Number(handle.getAttribute('aria-valuenow')) || 50;
    if (e.key === 'ArrowLeft') pos(cur - 5);
    else if (e.key === 'ArrowRight') pos(cur + 5);
    else if (e.key === 'Home') pos(0);
    else if (e.key === 'End') pos(100);
    else return;
    e.preventDefault();
  });
}

document.querySelectorAll('.cmp').forEach(initComparison);
```

## Notes

- **Touch support:** `pointerdown/move/up` unify touch, pen, and mouse input.
  `touch-action: none` prevents horizontal page scroll from stealing the drag,
  and `setPointerCapture` keeps tracking when the finger drifts off-element.
- **Accessibility:** the handle is keyboard-focusable with `role="slider"`;
  Arrow keys move in 5% steps, Home/End jump to the edges, and
  `aria-valuenow`/`aria-valuetext` announce the current position.
- **Fallback:** with JS disabled the clip defaults show only the top image at
  its natural size — keep the inline `--pos: 50%` so SSR output looks right
  before hydration.
- Use images with identical dimensions; mismatched aspect ratios make the
  layers drift out of alignment while clipping.
