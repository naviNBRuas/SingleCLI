# Page Transition Pattern

A reusable page-transition pattern for Next.js App Router projects: uses the
native View Transitions API where supported, falls back to a GSAP overlay
wipe everywhere else. Respects `prefers-reduced-motion` by navigating
instantly with no transition at all.

Flow: `TransitionLink` intercepts internal navigation; if
`document.startViewTransition` exists, the router push is wrapped in it and
the browser crossfades for free; otherwise a full-screen GSAP overlay wipes
in, the route swaps behind it, and the overlay wipes out after mount.

## Implementation sketch

```tsx
// components/page-transition.tsx
"use client";

import { useRouter, usePathname } from "next/navigation";
import { useCallback, useRef } from "react";
import gsap from "gsap";

const EASE = "power2.inOut";

const supportsViewTransitions = () =>
  typeof document !== "undefined" && "startViewTransition" in document;

const prefersReducedMotion = () =>
  typeof window !== "undefined" &&
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function usePageTransition() {
  const router = useRouter();
  const pathname = usePathname();
  const overlayRef = useRef<HTMLDivElement | null>(null);
  const busy = useRef(false);

  const transitionTo = useCallback(
    async (href: string) => {
      if (busy.current || href === pathname) return;

      // Reduced motion: instant navigation, no transition.
      if (prefersReducedMotion()) return router.push(href);

      // Modern browsers: native cross-fade via the View Transitions API.
      if (supportsViewTransitions()) {
        document.startViewTransition(() => router.push(href));
        return;
      }

      // Fallback: GSAP overlay wipe. Animate in, swap route, animate out.
      busy.current = true;
      const overlay = overlayRef.current;
      if (!overlay) return router.push(href);

      await gsap.to(overlay, { scaleY: 1, duration: 0.35, ease: EASE });
      router.push(href);
      await gsap.to(overlay, {
        scaleY: 0,
        duration: 0.35,
        ease: EASE,
        delay: 0.15,
      });
      busy.current = false;
    },
    [pathname, router]
  );

  return { transitionTo, overlayRef };
}

export function PageTransitionShell({ children }: { children: React.ReactNode }) {
  const { overlayRef } = usePageTransition();
  return (
    <>
      {children}
      <div
        ref={overlayRef}
        aria-hidden="true"
        style={{ transform: "scaleY(0)", transformOrigin: "bottom", position: "fixed", inset: 0, zIndex: 9999 }}
      />
    </>
  );
}

export function TransitionLink({ href, children }: { href: string; children: React.ReactNode }) {
  const { transitionTo } = usePageTransition();
  return (
    <a
      href={href}
      onClick={(e) => {
        e.preventDefault();
        transitionTo(href);
      }}
    >
      {children}
    </a>
  );
}
```

## Usage

1. Wrap the app in `PageTransitionShell` inside your root layout's `<body>`.
2. Replace internal `next/link` usages with `TransitionLink`.
3. Optionally style the native path with CSS:

```css
@media (prefers-reduced-motion: no-preference) {
  ::view-transition-old(root) { animation: fade-out 200ms ease both; }
  ::view-transition-new(root) { animation: fade-in 200ms ease both; }
}
```

## Reduced motion

The hook checks `prefers-reduced-motion` before animating and short-circuits
to a plain `router.push` — instant navigation. The CSS above also gates the
`::view-transition` animations behind `no-preference`, so native transitions
are skipped for those users too.

## Performance notes

- Never block navigation on a long animation. Keep each phase under ~400ms;
  if content loads slowly, navigate first and reveal after mount.
- `startViewTransition` snapshots the old DOM — avoid triggering it on very
  large pages mid-scroll if profiling shows jank.
- Animate only `transform`/`opacity` in the wipe, never layout properties.
- Debounce rapid clicks with the `busy` ref so overlapping transitions can't
  stack animations or double-push history entries.
