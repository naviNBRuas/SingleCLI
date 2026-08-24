# Cinematic Hero

Full-viewport, film-grade opening section: layered parallax background, a slow
push-in zoom, staggered title reveal via GSAP, and a scroll cue. Built for
premium launch pages that need to set tone in under three seconds.

## When to use / when not

Use for launch or brand pages where emotional impact outranks information
density and you have a strong visual asset. Avoid on docs, dashboards, or any
SEO-critical page that needs LCP under 1.5s on mid-tier mobile.

## Implementation sketch

```tsx
import { useLayoutEffect, useRef } from "react";
import gsap from "gsap";
import { ScrollTrigger } from "gsap/ScrollTrigger";
gsap.registerPlugin(ScrollTrigger);

type Props = {
  eyebrow?: string;
  title: string;
  cta: { label: string; href: string };
  media: { src: string; poster: string };
};

export function CinematicHero({ eyebrow, title, cta, media }: Props) {
  const root = useRef<HTMLElement>(null);

  useLayoutEffect(() => {
    const ctx = gsap.context(() => {
      // Plate: one-shot cinematic push-in (oversized, so edges never show)
      gsap.fromTo(".hero__plate", { scale: 1.12 },
        { scale: 1, duration: 8, ease: "power2.out" });

      // Content: staggered rise
      gsap.from(".hero__reveal", { y: 48, opacity: 0, duration: 0.9,
        ease: "power3.out", stagger: 0.12, delay: 0.25 });

      // Parallax scrub while the hero scrolls out
      gsap.to(".hero__plate", { yPercent: 12, ease: "none",
        scrollTrigger: { trigger: root.current, start: "top top",
          end: "bottom top", scrub: true } });
    }, root);

    return () => ctx.revert(); // StrictMode-safe cleanup
  }, []);

  return (
    <section ref={root} className="hero">
      <div className="hero__media" aria-hidden="true">
        <div className="hero__plate"
          style={{ backgroundImage: `url(${media.src})` }} />
        <div className="hero__scrim" />
      </div>
      <div className="hero__content">
        {eyebrow && <p className="hero__eyebrow hero__reveal">{eyebrow}</p>}
        <h1 className="hero__title hero__reveal">{title}</h1>
        <a className="hero__cta hero__reveal" href={cta.href}>{cta.label}</a>
      </div>
      <span className="hero__scroll-cue" aria-hidden="true" />
    </section>
  );
}
```

## CSS essentials

```css
.hero { position: relative; min-height: 100svh;
  display: grid; place-items: center; overflow: clip; }
.hero__plate {
  position: absolute; inset: -6%; /* headroom so the zoom never exposes edges */
  background-size: cover; background-position: center;
  will-change: transform;
}
.hero__scrim { position: absolute; inset: 0;
  /* bottom-heavy gradient holds copy contrast over bright plates */
  background: linear-gradient(180deg, rgb(0 0 0 / .35) 0%,
    rgb(0 0 0 / 0) 40%, rgb(0 0 0 / .65) 100%); }
```

## Responsive behavior

| Viewport | Behavior |
| --- | --- |
| ≥ 1024px | Full treatment: GSAP parallax + push-in, video or still plate |
| 768–1023px | Same layout; parallax only, poster still replaces video |
| < 768px | Light fallback: static plate, CSS-only fade-in, no GSAP timeline |

Gate the heavy path in JS and swap assets with `srcset`/`<picture>` so mobile
never downloads the desktop plate:

```ts
const light = matchMedia("(max-width: 767px)").matches;
if (!light) buildHeroTimeline(root.current);
```

## Accessibility

- Honor `prefers-reduced-motion: reduce`: bail before building any timeline
  and paint the final composed state immediately:

  ```ts
  if (matchMedia("(prefers-reduced-motion: reduce)").matches) return;
  ```

- Disable looping cues (scroll indicator) under the same media query.
- Media layer is `aria-hidden="true"`; the `<h1>` stands alone semantically.
- Contrast: the scrim must hold ≥ 4.5:1 against the darkest expected frame,
  not just the poster image.
- Scroll cue stays decorative and non-focusable; the CTA is a plain link.

## Performance considerations

- The plate is the LCP element: preload it with `fetchpriority="high"`,
  ship AVIF/WebP, and cap served width around 2560px.
- Video plates: muted, `playsinline`, `preload="metadata"`, ≤ 4s loop,
  paused when the tab hides or the hero scrolls out of view.
- Animate compositor-only properties only: `transform` and `opacity`.
- One GSAP context per hero; revert it on unmount to avoid ticker leaks.

## Usage

```tsx
<CinematicHero eyebrow="NBR Vault"
  title="Infrastructure you can read like prose."
  cta={{ label: "Get started", href: "/start" }}
  media={{ src: "/plates/dune.avif", poster: "/plates/dune.jpg" }} />
```
