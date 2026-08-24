# Video Background Hero

Full-viewport hero with an autoplaying, muted, looping background video and a gradient scrim so overlaid copy stays legible on any frame. A poster image covers slow networks, reduced-motion users, and failed loads.

## When to use

- Marketing landing pages where ambient motion reinforces brand feel.
- Heroes whose video is decorative, not informational.

Do **not** use this for content users must actually watch — that belongs in a real `<video controls>` player. The background track is decorative (`aria-hidden="true"`) and carries no information, so nothing is lost if it never plays.

## Implementation sketch

```html
<section class="hero" aria-label="Introduction">
  <video class="hero__video" autoplay muted loop playsinline preload="none"
    poster="/img/hero-poster.jpg" aria-hidden="true">
    <source data-src="/video/hero.webm" type="video/webm" />
    <source data-src="/video/hero.mp4" type="video/mp4" />
  </video>
  <div class="hero__scrim" aria-hidden="true"></div>
  <div class="hero__content">
    <h1>Ship faster</h1>
    <a class="hero__cta" href="/signup">Get started</a>
  </div>
</section>
```

```css
.hero {
  position: relative;
  min-height: 100svh; /* avoids iOS URL-bar jumpiness of 100vh */
  display: grid;
  place-items: center;
  overflow: hidden;
  color: #fff;
}
.hero__video,
.hero__scrim { position: absolute; inset: 0; width: 100%; height: 100%; }
.hero__video { object-fit: cover; background: #000; }
.hero__scrim {
  /* Darken top/bottom so white copy reads over any frame */
  background: linear-gradient(to bottom,
    rgb(0 0 0 / 0.55), rgb(0 0 0 / 0.15) 45%, rgb(0 0 0 / 0.65));
}
.hero__content { position: relative; z-index: 1; text-align: center; }
```

```js
const video = document.querySelector('.hero__video');
const reduceMotion = matchMedia('(prefers-reduced-motion: reduce)').matches;
if (reduceMotion) {
  video.remove(); // poster-only static hero
} else {
  // Lazy-load: swap data-src in only as the hero nears the viewport
  new IntersectionObserver(([e], obs) => {
    if (!e.isIntersecting) return;
    video.querySelectorAll('source').forEach((s) => (s.src = s.dataset.src));
    video.load(); // starts fetching; autoplay + loop take over from here
    obs.disconnect();
  }, { rootMargin: '200px' }).observe(video);
  // Stop decoding frames in hidden tabs
  document.addEventListener('visibilitychange', () => {
    document.hidden ? video.pause() : video.play().catch(() => {});
  });
}
```

## Usage

1. Keep the clip 5–10s, fully silent, and a seamless loop.
2. Encode WebM (VP9/AV1) first, H.264 MP4 second; 720p suffices behind a scrim.
3. Always ship a `poster` — it doubles as the reduced-motion and error state.
4. `muted` + `playsinline` are mandatory: browsers refuse otherwise-audible autoplay, and iOS Safari needs `playsinline` to stay inline.

## Performance

- Lazy-load any hero below the fold: sources stay in `data-src` and swap in via IntersectionObserver; keep `preload="none"` until then.
- Compress hard: target ≤ 1–2 MB combined across both encodes.
- Pause on `visibilitychange` (above) so background tabs stop burning CPU and battery.

## Reduced-motion fallback

With `prefers-reduced-motion: reduce`, remove the `<video>` element entirely and let the poster image stand alone: identical layout, zero motion, zero video bytes downloaded.

## Mobile data considerations

- Honor `navigator.connection.saveData`: serve the poster only and skip the source swap-in.
- On slow connections (`effectiveType` of 2g/slow-2g), treat the poster as the default — silent autoplay video can drain a capped data plan invisibly.
- Never preload on mobile; defer every video byte until the hero approaches the viewport.
