# Cursor Follower

A custom cursor pattern: a styled `div` trails the pointer with lerp smoothing driven by one `requestAnimationFrame` loop, then scales/morphs over interactive elements (`a`, `button`, `[data-cursor]`). The native cursor is kept on touch devices and only replaced on fine-pointer devices.

## Behavior contract

- One follower element per page, `aria-hidden="true"`, `pointer-events: none`.
- Follows the pointer with exponential smoothing (lerp factor ~0.15–0.25).
- Grows/blends when hovering links, buttons, and elements with `data-cursor`.
- Removed entirely when `(hover: none) and (pointer: coarse)` matches — native touch behavior wins.

## Styles

Markup is a single element near the end of `<body>`:

```html
<div class="cursor-follower" aria-hidden="true"></div>
```

```css
.cursor-follower {
  position: fixed;
  top: 0;
  left: 0;
  width: var(--cursor-size, 14px);
  height: var(--cursor-size, 14px);
  border-radius: 50%;
  border: 1px solid rgb(255 255 255 / 0.9);
  background: transparent;
  mix-blend-mode: difference;
  pointer-events: none;
  z-index: 9999;
  transform: translate3d(var(--cursor-x, -100px), var(--cursor-y, -100px), 0)
    translate(-50%, -50%) scale(var(--cursor-scale, 1)); /* position is JS-driven per frame — never transition it */
}

.cursor-follower.is-active { --cursor-scale: 2.6; background: rgb(255 255 255 / 0.15); }
```

## Vanilla implementation sketch

```js
export function initCursorFollower() {
  const el = document.querySelector(".cursor-follower");
  if (!el) return () => {};

  const fine = matchMedia("(hover: hover) and (pointer: fine)");
  if (!fine.matches) {
    el.remove(); // touch device: fall back to native cursor entirely
    return () => {};
  }

  const target = { x: -100, y: -100 };
  const current = { ...target };
  const LERP = 0.18;

  // Event delegation: one listener instead of N per interactive element.
  const onMove = (e) => { target.x = e.clientX; target.y = e.clientY; };
  const onOver = (e) =>
    el.classList.toggle("is-active", Boolean(e.target.closest("a, button, [data-cursor]")));

  document.addEventListener("pointermove", onMove, { passive: true });
  document.addEventListener("pointerover", onOver, { passive: true });

  let rafId = 0;
  const tick = () => {
    current.x += (target.x - current.x) * LERP;
    current.y += (target.y - current.y) * LERP;
    el.style.setProperty("--cursor-x", `${current.x.toFixed(1)}px`);
    el.style.setProperty("--cursor-y", `${current.y.toFixed(1)}px`);
    rafId = requestAnimationFrame(tick);
  };
  rafId = requestAnimationFrame(tick);

  return () => {
    cancelAnimationFrame(rafId);
    document.removeEventListener("pointermove", onMove);
    document.removeEventListener("pointerover", onOver);
    el.remove();
  };
}
```

## Usage

1. Add the `.cursor-follower` div once, near the end of `<body>` (or portal it last in React).
2. Call `initCursorFollower()` after mount; keep the returned disposer and call it on unmount/route change.
3. Tag elements that need a distinct morph with `data-cursor="link" | "text" | "drag"` and style variants off that attribute.
4. Optionally hide the native dot cursor with `html.has-custom-cursor { cursor: none }` — add the class only after the fine-pointer check passes, so keyboard/touch users never lose their pointer.

## React usage without re-renders

Never store mouse coordinates in component state — a `mousemove` at 120–240 Hz would re-render the tree per event. Instead:

- Keep the follower as a plain DOM node via `ref` and mutate it directly (same loop as above), or drive position purely through CSS custom properties (`el.style.setProperty`) so no React reconciliation runs.
- Mount/unmount with `useEffect(() => initCursorFollower(), [])` and call the returned cleanup.

## Performance notes

- Exactly one `requestAnimationFrame` loop; all reads/writes batched per frame.
- Write-only updates: never read layout (`getBoundingClientRect`) inside the loop — avoids forced synchronous layout thrashing.
- Drive position with `translate3d`/custom properties so movement stays compositor-only (never animate `top`/`left`); `{ passive: true }` listeners so scrolling is never blocked.
- Pause the loop when `document.hidden`; cancel the rAF id on cleanup to prevent leaks across SPA route changes.
- Delegate hover detection (`pointerover` + `closest()`) instead of per-element listeners; cheaper on large/dynamic DOM.
- `mix-blend-mode: difference` forces compositing of overlapping layers; test on low-end GPUs and gate behind a media query if needed.

## Accessibility

- Decorative only (`aria-hidden="true"`); before hiding the native cursor for magnifier users, ship a high-contrast variant via `prefers-contrast: more`.
- Respect `prefers-reduced-motion`: skip the lerp trail and snap directly to the pointer position.
