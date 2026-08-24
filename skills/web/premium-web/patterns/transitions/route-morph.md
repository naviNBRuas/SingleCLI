# Route Morph: Shared-Element Route Transitions

A clicked card or image visually morphs into the next page's hero element using the
View Transitions API. Browsers without support fall back to a plain Next.js client-side
navigation — instant, unstyled, nothing breaks.

## How it works

1. Both elements (list card and detail hero) share a unique `view-transition-name`.
2. Click is intercepted; the route push runs inside `document.startViewTransition()`.
3. The browser snapshots old and new states and tweens matching names between them —
   position, size, and cross-fade come for free from the default transition group.

## Implementation

```tsx
// lib/use-route-morph.ts
"use client";
import { useRouter } from "next/navigation";
import type { MouseEvent } from "react";

const reducedMotion = () =>
  window.matchMedia("(prefers-reduced-motion: reduce)").matches;

export function useRouteMorph() {
  const router = useRouter();
  return (e: MouseEvent<HTMLAnchorElement>, href: string) => {
    e.preventDefault();
    const startVT = (
      document as Document & {
        startViewTransition?: (cb: () => Promise<void>) => void;
      }
    ).startViewTransition?.bind(document);

    // Fallback path: no View Transitions support or reduced motion requested.
    if (!startVT || reducedMotion()) {
      router.push(href);
      return;
    }

    startVT(async () => {
      router.push(href);
      // Hold the snapshot until React commits the new route.
      await new Promise<void>((r) => requestAnimationFrame(() => r()));
    });
  };
}
```

```tsx
// components/morph-link.tsx
export function MorphLink({ href, name, children }: {
  href: string;
  name: string; // identical on the source card and the target hero
  children: React.ReactNode;
}) {
  const navigate = useRouteMorph();
  return (
    <a href={href} style={{ viewTransitionName: name }} onClick={(e) => navigate(e, href)}>
      {children}
    </a>
  );
}
```

## Usage

Match names across routes; scope them per item slug so pairs stay unique:

```tsx
// Grid item
<MorphLink href={`/work/${slug}`} name={`hero-${slug}`}>
  <Image src={cover} alt={title} />
</MorphLink>

// app/work/[slug]/page.tsx
<h1 style={{ viewTransitionName: `hero-${slug}` }}>{title}</h1>
<Image src={cover} alt="" style={{ viewTransitionName: `art-${slug}` }} />
```

Only one visible element may own a given name at a time. On long lists, gate names to
the hovered/focused card instead of stamping every item eagerly.

## Reduced motion

The hook returns early for `prefers-reduced-motion: reduce`, so those users get an
instant jump. Add CSS as a second layer so nothing tweens even if a transition starts:

```css
@media (prefers-reduced-motion: reduce) {
  ::view-transition-group(*), ::view-transition-old(*), ::view-transition-new(*) {
    animation: none !important;
  }
}
```

## Browser support

- Same-document transitions are Chromium-first: Chrome/Edge 111+, Brave, Arc, Opera.
- Safari 18+ supports the API; Firefox support landed late/partially — never assume it.
- Everywhere else `document.startViewTransition` is simply `undefined`; the feature
  check in the hook routes those users straight to `router.push`. Degrade gracefully:
  no UA sniffing, no polyfill required, no layout shift when the animation is absent.
