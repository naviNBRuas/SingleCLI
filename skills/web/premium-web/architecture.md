# Frontend Architecture for Premium Websites

Architecture decisions for high-end marketing sites, product launches, and
brand experiences where motion quality and perceived performance are the
product. Optimized for sites in the 5–30 page range with heavy scroll and
entrance animation budgets.

## Framework choice: Next.js vs Vite vs plain React

### Use Vite + React when

- The site has **no SEO-sensitive dynamic content** beyond static pages you
  can prerender at build time.
- There are no per-request server concerns: no auth walls, no ISR, no
  personalization.
- The site is deployed as static files (S3/CloudFront, Netlify, GitHub Pages)
  or behind a CDN with an HTML fallback.
- You want the smallest possible toolchain surface and fastest cold builds.

For most premium marketing sites this is the right call. Static export keeps
TTFB low everywhere, and premium sites rarely need server logic — they need
fast assets and good caching.

### Use Next.js when

- Pages need **per-request data** (CMS-driven pricing, localized content,
  A/B variants resolved server-side).
- You want incremental static regeneration for CMS-edited pages so editors
  publish without a rebuild pipeline.
- The project shares a monorepo with a real app that already uses Next.js —
  consistency usually wins over theoretical purity.
- You need image optimization infrastructure (`next/image`) across many
  editorially-uploaded assets.

If you do use Next.js on an animation-heavy site, prefer SSG with `revalidate`
over SSR for everything that can be static; hydration cost is the enemy of
smooth first-load animations.

### Plain React (no meta-framework)

Only justified when embedding into an existing host (Rails, Django, WordPress
admin surfaces). Do not choose it for greenfield standalone sites — you lose
prerendering and code-splitting ergonomics for no benefit.

### Decision rule

> Default to Vite + React with full static prerender. Move to Next.js only
> when a concrete server-side requirement exists, not "we might later."

## Component architecture for animation-heavy pages

The classic failure mode: one `Hero.tsx` file containing layout JSX, GSAP
timelines, scroll listeners, resize handlers, and copy — 400+ lines, untestable,
and re-rendering on every animation frame.

### Principles

1. **Structure components by visual section, not by technology.** A page is a
   composition of sections (`<Hero/>`, `<FeatureMarquee/>`, `<PricingTable/>`),
   each owning its own entrance choreography. Sections never reach into each
   other's DOM.
2. **Separate three layers inside each section:**
   - `Section.tsx` — layout, semantic markup, copy. No animation imports.
   - `useSectionAnimation.ts` — timeline construction, cleanup, reduced-motion
     handling.
   - Presentational children (`<StatCard/>`, `<LogoRow/>`) — pure markup +
     props, animatable via refs passed down or data-attributes selected up.
3. **Animate via refs and data attributes, never global selectors.**
   `gsap.utils.toArray('[data-animate]')` scoped to a section root, not the
   document. This keeps sections independently mountable and testable.
4. **Co-locate timelines with the component that owns the DOM they target.**
   If two sections must coordinate (e.g., pinned hero hands off to next
   section), coordinate through a shared hook or context — not by one
   component querying another's nodes.

### File shape per section

```
sections/hero/
  Hero.tsx              # markup + copy, ~80 lines max
  use-hero-animation.ts # all GSAP/scroll logic
  index.ts
```

## Animation abstraction pattern

Never scatter raw `gsap.timeline()` calls through components. Centralize
motion behavior behind hooks so timing language stays consistent and
reduced-motion is enforced once.

### The `useEntranceAnimation` pattern

```tsx
// One place defines what "enter" means for this site:
// duration scale, easing vocabulary, stagger defaults.
export function useEntranceAnimation(
  scope: RefObject<HTMLElement>,
  opts?: EntranceOptions
) {
  const prefersReduced = useReducedMotion();

  useEffect(() => {
    if (!scope.current) return;
    if (prefersReduced) {
      // Snap to final state; skip all motion.
      gsap.set(scope.current.querySelectorAll('[data-animate]'), { opacity: 1 });
      return;
    }
    const ctx = gsap.context(() => {
      const targets = scope.current!.querySelectorAll('[data-animate]');
      gsap.from(targets, {
        y: 24, opacity: 0, duration: 0.7,
        ease: 'power3.out',
        stagger: opts?.stagger ?? 0.08,
        delay: opts?.delay ?? 0,
        clearProps: 'transform,opacity',
      });
    }, scope);
    return () => ctx.revert();
  }, [scope, prefersReduced]);

  return scope;
}
```

Usage in a section:

```tsx
const ref = useRef<HTMLDivElement>(null);
useEntranceAnimation(ref);
return (
  <section ref={ref}>
    <h1 data-animate>{headline}</h1>
    <p data-animate>{subcopy}</p>
  </section>
);
```

Why this works well:

- **One easing/duration vocabulary** — swap `power3.out` site-wide from one file.
- **Reduced motion is not opt-in per component** — impossible to forget.
- **Cleanup is automatic** — `gsap.context().revert()` prevents leaked
  timelines on unmount and React StrictMode double-invocations.
- **Components stay declarative** — `data-animate` marks intent; the hook owns
  execution. Reviewers read markup without tracing JS.

Extend the same pattern for scroll-triggered reveals (`useScrollReveal`) and
pinned sequences (`usePinnedSequence`). Three hooks cover ~90% of premium-site
motion needs; anything more exotic stays in the section's own hook file.

## Project structure conventions

```
src/
  main.tsx / routes.tsx
  styles/
    tokens.css        # design tokens only (colors, spacing, type scale)
    base.css          # resets, element defaults
    utilities.css     # minimal utility classes, no framework soup
  components/
    ui/               # generic primitives: Button, Badge, Container
    layout/           # Header, Footer, PageShell
  sections/           # page-level animated compositions
    hero/
    feature-grid/
    pricing/
  hooks/
    use-entrance-animation.ts
    use-scroll-reveal.ts
    use-pinned-sequence.ts
    use-reduced-motion.ts
  lib/
    motion-tokens.ts  # easings, durations, stagger constants
    analytics.ts
  pages/              # thin route files composing sections
```

Rules that keep it premium-grade:

- **Sections own their animations; hooks own the how.** Nothing imports GSAP
  outside `hooks/` and `sections/*/use-*.ts`.
- **No CSS-in-JS for animated properties.** Transform/opacity live in GSAP or
  transitions defined once; avoid fighting style recalculation mid-timeline.
- **Route files are compositions, not implementations.** If a page file
  exceeds ~40 lines, logic belongs in a section.
- **Design tokens before Tailwind-style ad hoc values.** Premium feel comes
  from consistency; centralize spacing/easing/color scales and reference them.
