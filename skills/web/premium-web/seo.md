# SEO for Animated / JS-Heavy Premium Websites

Premium sites live on motion: scroll-triggered reveals, parallax layers,
canvas scenes, page transitions. None of that is a problem for search
engines as long as the *content* itself is crawlable, renderable, and fast.
This skill keeps an animation-driven site fully indexable without giving up
the visual experience.

## Never hide real content from crawlers

Google renders with a real browser (evergreen Chromium), so JS-driven
reveals are usually seen eventually — but rendering happens days after
crawling and can fail, other consumers (social scrapers, AI agents) don't
run JS at all, and content revealed only after deep scroll may be treated
as low priority or missed entirely.

Practical rules:

- Server-render (SSR) or pre-render (SSG) every meaningful text node into
  the initial HTML. Framer Motion, GSAP, and Three.js should animate
  *existing* DOM, not inject it client-side after hydration.
- Never make `display:none` + IntersectionObserver the only path to
  visibility. Content may start hidden, but it must already be in the DOM
  and readable without JS.
- Don't remove whole sections from layout to "reveal" them later; animate a
  wrapper's `transform`/`opacity` instead of hiding the content itself.
- If a heavy hero canvas hydrates late, ship its copy as static HTML beside
  it rather than inside it.

### Scroll-triggered reveals, done right

```html
<!-- Bad: empty until JS runs -->
<div class="reveal"></div>

<!-- Good: content present, JS only animates it -->
<section class="reveal">
  <h2>Our craft</h2>
  <p>Fully server-rendered copy.</p>
</section>
```

Default state is visible; reveal styles only apply when a `.js` class (set
by an inline head script: `document.documentElement.classList.add('js')`)
proves scripting works:

```css
.js .reveal { opacity: 0; transform: translateY(24px); }
.js .reveal.is-visible { opacity: 1; transform: none; transition: 600ms ease; }
@media (prefers-reduced-motion: reduce) {
  .js .reveal { opacity: 1; transform: none; }
}
```

## SSR vs SSG vs CSR

| Approach | Use when | SEO note |
|---|---|---|
| SSG (Astro, Eleventy, Next export) | Marketing pages, portfolios | Best TTFB, trivially crawlable |
| SSR (Next, Remix, Nuxt) | Dynamic or personalized data | Streaming must not delay meta tags |
| CSR-only | Rarely defensible here | Needs prerendering fallbacks; last resort |

For animation-heavy pages, SSG plus partial hydration is the sweet spot: a
static shell paints instantly while motion bundles load progressively.

## Meta tags

- Unique `<title>` (50–60 chars) and `meta description` (140–160 chars) per
  route, emitted from the server.
- Canonical link per route, especially with query params or filters around.
- Open Graph (`og:title`, `og:description`, 1200×630 `og:image`, `og:url`)
  and Twitter cards — social scrapers do **not** execute JS.
- Viewport meta, `lang` attribute, favicon/app-icon set.
- Client-side route transitions must update title/canonical/OG and every URL
  must remain statically servable, shareable, and linkable.

## Structured data (JSON-LD)

Emit JSON-LD during SSR/SSG, never after hydration. Typical types:

- `Organization` or `LocalBusiness` site-wide; `WebSite` (+ `SearchAction`
  if you have site search)
- `Article`, `Product`, `Service`, or `CreativeWork`/`CollectionPage` for
  case studies and portfolio entries
- `BreadcrumbList` wherever nav implies hierarchy

Validate with the Rich Results Test and the schema.org validator, and keep
markup consistent with visible content — mismatches draw penalties.

## Core Web Vitals are ranking factors

**LCP ≤ 2.5s at p75.** Hero media is usually the LCP element: preload it
with `fetchpriority="high"`, serve AVIF/WebP sized responsively, and never
lazy-load it or wrap it in an entrance animation.

**INP ≤ 200ms at p75.** Break long tasks, defer scene setup with
`requestIdleCallback`, debounce scroll handlers, and stick to
compositor-friendly properties (`transform`, `opacity`).

**CLS ≤ 0.1.** Reserve space with `aspect-ratio` or explicit dimensions,
pair `font-display: swap` with metric-compatible fallbacks, and don't let
late-injected banners push content around.

Rankings use field data (CrUX): monitor the Search Console CWV report and
treat Lighthouse as a debugging aid, not the score that counts.

## Semantic HTML in a highly visual page

Animation is decoration; structure carries meaning.

- One `<h1>` per page; logical heading order even when heavily restyled.
- Real landmarks: `<header>`, `<nav>`, `<main>`, labelled `<section>`s,
  `<footer>`.
- Navigation is real `<a href>` elements — never div-with-onclick; view
  transitions must preserve URLs and working back/forward.
- Words that matter exist as text, not pixels inside canvas or video;
  mirror essential scene text with `aria-label` or visually-hidden copy.
- Mark decorative layers `aria-hidden="true"`; keep anything interactive
  keyboard-focusable with visible focus styles.

## Pre-launch audit

1. `curl` each key URL — every meaningful sentence is in the raw HTML.
2. Disable JS: content and navigation still work.
3. Search Console URL Inspection matches the intended rendered DOM.
4. Field CWV pass on mobile, sitemap submitted, OG previews render cleanly.
