# Responsive Strategy for Animated / 3D Premium Sites

Responsive design on premium sites is not just about layout reflow — it is about
*degrading experience tiers* deliberately. A hero built around WebGL shaders,
particle fields, and scroll-linked camera work cannot simply shrink; each device
class needs an explicit capability contract.

## Core principle

Design top-down (full fidelity), then degrade by *feature budget*, not by
screen width alone. Width decides layout; capabilities decide effects.

---

## Degradation table

| Capability | Desktop (≥1024px) | Tablet (768–1023px) | Mobile (<768px) |
|---|---|---|---|
| WebGL scene | Full shader pipeline, PBR materials, post-processing (bloom/DOF) | Same scene, half-res render target, post-processing off | Static pre-rendered frame or CSS 3D fallback |
| Particle count | 20k–100k GPU particles | 5k–15k | 0 — replace with 2–3 layered PNG/SVG sprites animated via CSS |
| Scroll experience | Scroll-jacked pinned sections, smooth-scroll (Lenis/locomotive) | Native scroll, reduced pin durations | Fully native scroll, no hijacking |
| Video | Autoplay 1080p+ ambient loops | Autoplay 720p, `playsinline` | Poster image + tap-to-play only |
| Cursor effects | Custom cursor, magnetic hover, parallax tilt | Disabled — pointer is coarse | Disabled |
| Text animation | Per-char split-text staggered reveals | Per-word reveals | Whole-block fade/slide only |
| Frame budget | 60fps @ 16.6ms | 60fps but simpler scenes | No rAF loop on load path; animate on interaction |
| Model complexity | ≤500k tris, DRACO-compressed | ≤150k tris | No runtime models |

### Degradation ladder (implementation order)

1. Ship desktop tier first as the reference implementation.
2. Gate every effect behind a capability check — never behind a media query alone.
3. Tablet = same engine, lower budgets (`dpr` cap, fewer particles).
4. Mobile = swap the *renderer*, not the settings: static art + CSS motion.
5. Every degraded tier must be a designed state, not a broken one.

---

## Touch vs. pointer interaction

Pointer events unify mouse/pen/touch, but intent differs:

| Concern | Pointer (mouse) | Touch |
|---|---|---|
| Hover states | Central to affordance (underline reveals, tooltips) | Meaningless — never rely on them to expose content |
| Press feedback | `mouseenter`/`mouseleave` transitions | `touchstart` scale/opacity within ~120ms, release completes |
| Drag / orbit controls | Inertia-heavy, right-click zoom | One-finger rotate, two-finger zoom, must not trap page scroll |
| Custom cursor | Yes (`cursor: none` + follower) | Never render a fake cursor |
| Magnetic buttons | Offset toward pointer | Off — use active-state elevation instead |
| Tap targets | ≥32px acceptable | ≥44×44px (Apple HIG), ≥48dp (Material) |

Practical rules:

- Feature-detect input, don't infer from width: `(hover: none) and (pointer: coarse)`.
- On touch, replace hover-revealed content with always-visible or tap-to-toggle UI.
- For WebGL canvases: set `touch-action: pan-y` so vertical scroll stays native;
  reserve full gesture capture (`none`) only for dedicated viewer modes.
- Debounce/ignore synthetic mouse events fired after touchend to avoid double handlers.
- Respect `prefers-reduced-motion` at every tier — it overrides all of the above.

```js
const fine = matchMedia('(hover: hover) and (pointer: fine)').matches;
if (fine) initCursorFollower();
```

---

## Viewport-based feature detection patterns

Never branch on `window.innerWidth` alone. Combine viewport, DPR, and hardware
signals into one resolved "tier":

```js
function resolveTier() {
  const w = window.innerWidth;
  const dpr = Math.min(window.devicePixelRatio || 1, 2);
  const cores = navigator.hardwareConcurrency ?? 4;
  const mem = navigator.deviceMemory ?? 4; // GB, Chrome-only
  const reduced = matchMedia('(prefers-reduced-motion: reduce)').matches;

  if (reduced) return 'static';
  if (w >= 1024 && dpr <= 2 && cores >= 6 && mem >= 6) return 'desktop';
  if (w >= 768 && cores >= 4) return 'tablet';
  return 'mobile';
}
```

Supporting checks:

- **WebGL support**: create a context, verify `WEBGL_lose_context` recovery and
  `MAX_TEXTURE_SIZE >= 4096`; fall back if creation throws.
- **Connection**: `navigator.connection.effectiveType` — downgrade video/particles
  on `2g`/`slow-2g`, defer non-critical assets on `saveData`.
- **Resize semantics**: listen to `matchMedia('(min-width: 768px)').addEventListener('change')`
  rather than debounced resize — tablets rotating need tier switches, not relayouts.
- **Visual viewport**: use `visualViewport` API when fixed overlays meet keyboards.
- **Rehydration rule**: tier changes may only add effects after idle
  (`requestIdleCallback`); never tear down mid-session except on `reduce`.

Cache the resolved tier once per session; re-evaluate only on orientation change
or explicit visibility return.
