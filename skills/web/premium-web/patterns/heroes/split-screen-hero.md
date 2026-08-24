# Split-Screen Hero

A two-column hero where one half carries the message (headline, copy,
primary CTA) and the other a visual (product screenshot, photo, or
muted video loop), separated by a subtle diagonal seam or animated
divider so the layout reads intentional rather than like two boxes.

## When to use

- Landing pages for products with a strong single visual asset.
- Feature announcements pairing copy with a screenshot or demo clip.
- Any hero that must communicate value in under five seconds.

Avoid when the visual asset is weak or generic stock — a split layout
magnifies mediocre imagery; use a centered hero instead.

## Implementation sketch

```html
<section class="split-hero">
  <div class="split-hero__copy">
    <p class="eyebrow">New</p>
    <h1>Ship faster with fewer moving parts</h1>
    <p class="lede">One tool replaces your four flaky scripts.</p>
    <a class="btn btn--primary" href="/signup">Start free</a>
  </div>
  <div class="split-hero__media" aria-hidden="true">
    <video autoplay muted loop playsinline poster="/hero-poster.webp">
      <source src="/hero-loop.webm" type="video/webm" />
    </video>
  </div>
</section>
```

```css
.split-hero {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  min-height: min(88vh, 760px);
}

/* Diagonal seam instead of a hard vertical edge */
.split-hero__media {
  clip-path: polygon(8% 0, 100% 0, 100% 100%, 0% 100%);
  margin-left: -4%; /* pull under copy column, no gap at the joint */
}

.split-hero__media video {
  width: 100%;
  height: 100%;
  object-fit: cover;
}
@media (prefers-reduced-motion: no-preference) {
  .split-hero__media {
    animation: reveal-seam 700ms ease-out both;
  }
  @keyframes reveal-seam {
    from { clip-path: polygon(100% 0, 100% 0, 100% 100%, 100% 100%); }
    to   { clip-path: polygon(8% 0, 100% 0, 100% 100%, 0% 100%); }
  }
}
```

For a static diagonal, keep the final `clip-path` value only. For an
accent-colored seam, layer a skewed pseudo-element between the columns.

## Usage

1. Copy on the left for LTR locales; `direction: rtl` mirrors it
   automatically.
2. Keep headlines short (~6–8 words); the narrow column punishes long
   ones.
3. Serve media at ≤ 1600px wide, WebP/AVIF preferred; lazy-init the
   video only when the hero is in view (IntersectionObserver).
4. Keep exactly one primary CTA; reserve media height via `aspect-ratio`
   to stop layout shift while the video loads.

## Mobile fallback

Below `768px`, stack vertically: copy first, media second, no clip-path
(it wastes vertical space and crops the subject badly).

```css
@media (max-width: 767px) {
  .split-hero {
    grid-template-columns: 1fr;
    min-height: auto;
  }
  .split-hero__media {
    clip-path: none;
    margin-left: 0;
    aspect-ratio: 4 / 3;
  }
}
```

If the media is a video, swap to a static poster on mobile unless the
clip is essential — autoplay on cellular is a cost and perf smell.

## Accessibility

- Mark decorative media `aria-hidden="true"`; never rely on the video to
  convey information the copy doesn't already state.
- Autoplay must be muted with `playsinline`; respect
  `prefers-reduced-motion` by showing the poster frame instead of
  animating or looping.
- Maintain ≥ 4.5:1 contrast for copy against its own background; don't
  let media bleed under text across the diagonal.
- Keyboard focus follows DOM order (copy → CTA → content); keep
  decorative media free of focusable elements.
