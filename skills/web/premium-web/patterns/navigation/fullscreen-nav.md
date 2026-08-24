# Fullscreen Takeover Navigation

A premium navigation pattern where the hamburger button triggers a fullscreen
overlay menu. Links reveal in a staggered sequence (GSAP), the overlay traps
focus while open, closes on `Escape`, and falls back to instant visibility when
the user prefers reduced motion.

## Anatomy

```html
<button class="nav-toggle" aria-expanded="false" aria-controls="fullscreen-nav">
  <span class="nav-toggle__line"></span>
  <span class="nav-toggle__line"></span>
</button>

<nav id="fullscreen-nav" class="fs-nav" hidden>
  <ul class="fs-nav__list">
    <li class="fs-nav__item"><a href="/work">Work</a></li>
    <li class="fs-nav__item"><a href="/about">About</a></li>
    <li class="fs-nav__item"><a href="/contact">Contact</a></li>
  </ul>
</nav>
```

Base CSS — overlay pinned to viewport, links initially shifted down + faded:

```css
.fs-nav {
  position: fixed;
  inset: 0;
  z-index: 100;
  display: grid;
  place-items: center;
  background: #0b0b0f;
}
.fs-nav__item a {
  display: inline-block;
  transform: translateY(40px);
  opacity: 0;
}
```

## Implementation sketch

```js
import gsap from "gsap";

const toggle = document.querySelector(".nav-toggle");
const nav = document.querySelector(".fs-nav");
const links = nav.querySelectorAll(".fs-nav__item a");
let isOpen = false;

function openNav() {
  isOpen = true;
  toggle.setAttribute("aria-expanded", "true");
  nav.hidden = false;

  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    gsap.set(links, { y: 0, opacity: 1 }); // reduced-motion fallback
    return;
  }

  gsap.to(nav, { opacity: 1, duration: 0.3 });
  gsap.fromTo(
    links,
    { y: 40, opacity: 0 },
    { y: 0, opacity: 1, stagger: 0.08, duration: 0.5, ease: "power3.out" }
  );
}

function closeNav() {
  isOpen = false;
  toggle.setAttribute("aria-expanded", "false");

  const done = () => {
    nav.hidden = true;
    toggle.focus(); // return focus to trigger
  };

  if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) {
    done();
    return;
  }

  gsap.to(links, {
    y: 20,
    opacity: 0,
    stagger: 0.04,
    duration: 0.25,
    onComplete: done,
  });
}

toggle.addEventListener("click", () => (isOpen ? closeNav() : openNav()));

document.addEventListener("keydown", (e) => {
  if (!isOpen) return;
  if (e.key === "Escape") closeNav();
  if (e.key === "Tab") trapFocus(e);
});

function trapFocus(e) {
  const focusables = [toggle, ...links];
  const first = focusables[0];
  const last = focusables[focusables.length - 1];

  if (e.shiftKey && document.activeElement === first) {
    e.preventDefault();
    last.focus();
  } else if (!e.shiftKey && document.activeElement === last) {
    e.preventDefault();
    first.focus();
  }
}
```

## Usage

1. Mount the markup near the end of `<body>` so the overlay paints above all
   page content without z-index fights.
2. Wire `openNav`/`closeNav` to your framework lifecycle (e.g. React refs +
   effects instead of direct DOM queries).
3. Tune the stagger (`0.08s`) to taste; keep total reveal under ~600ms.
4. On route change or link click, call `closeNav()` before navigating so the
   next page loads with the overlay dismissed and focus restored.

## Accessibility checklist

- Toggle carries `aria-expanded` (updated on every state change) and
  `aria-controls` pointing at the overlay id.
- Overlay starts `hidden`; never rely on `opacity: 0` alone, screen readers
  would still announce invisible links.
- Focus is trapped inside `[toggle, ...links]` while open; wrap-around via
  Shift+Tab handled explicitly.
- `Escape` closes from anywhere; focus returns to the toggle button.
- Respect `prefers-reduced-motion`: skip tweens entirely, show/hide instantly.

## Reduced motion notes

The `matchMedia("(prefers-reduced-motion: reduce)")` guard runs at open/close
time, not once at load — users can flip the OS setting mid-session. If you use
GSAP's global timeline elsewhere, alternatively set:

```js
gsap.globalTimeline.timeScale(reduced ? 100 : 1);
```

but per-call guards are clearer and avoid side effects on unrelated animations.

## Common pitfalls

- Forgetting to restore focus after close — keyboard users land back at `<body>`.
- Animating only `opacity` on the container while children stay transformed,
  leaving ghost text visible during exit.
- Not pausing background scroll; add `overflow: hidden` on `<body>` while open
  and remove it on close.
