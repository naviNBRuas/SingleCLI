# Skeleton Loader

Animated placeholder shapes that mirror the real content layout, shown
while data loads. Skeletons communicate structure before content arrives
and make waits feel shorter than spinners do.

## Problem

A blank page during a fetch feels broken, and spinners hide the layout.
Users cannot anticipate where content will appear, so perceived
performance drops even when actual latency is unchanged.

## Solution

Immediately render gray blocks shaped like the incoming content (avatar
circle, headline bar, paragraph lines), then swap them for real content
once the data resolves. A subtle shimmer marks the shapes as "loading."

## Implementation sketch

```html
<div class="card" aria-busy="true">
  <div class="skeleton avatar"></div>
  <div>
    <div class="skeleton line title"></div>
    <div class="skeleton line"></div>
    <div class="skeleton line short"></div>
  </div>
</div>
<p id="status" role="status" class="visually-hidden">Loading…</p>
```

```css
.skeleton {
  background: #e2e2e2;
  border-radius: 4px;
  position: relative;
  overflow: hidden;
}
.skeleton.avatar { width: 48px; height: 48px; border-radius: 50%; }
.skeleton.line   { height: 12px; margin: 8px 0; }
.skeleton.short  { width: 60%; }
.skeleton.title  { height: 18px; width: 80%; }
.skeleton::after {
  content: "";
  position: absolute;
  inset: 0;
  transform: translateX(-100%);
  background: linear-gradient(90deg, transparent, rgba(255,255,255,.6), transparent);
  animation: shimmer 1.4s infinite;
}
@keyframes shimmer {
  100% { transform: translateX(100%); }
}
```

```js
const container = document.querySelector(".card");
const res = await fetch("/api/article");
container.innerHTML = renderArticle(await res.json()); // swap skeletons out
container.setAttribute("aria-busy", "false");
document.getElementById("status").textContent = "Article loaded.";
```

## Usage

1. Match skeleton dimensions to the final content to avoid layout shift
   when swapping.
2. Set `aria-busy="true"` on the region while loading.
3. Replace skeleton nodes and set `aria-busy="false"` in the same frame.
4. Cap skeleton time at ~3 seconds; beyond that show progress or a retry.

## Accessibility

- `aria-busy="true"` on the container tells assistive tech the region is
  incomplete.
- One visually hidden live region (`role="status"`) announces the result
  once ("Article loaded."). Never announce each placeholder shape.
- Interactive elements ship only with real content, never as skeletons.

## Reduced motion

Under `prefers-reduced-motion`, disable the sweep and keep flat gray blocks — the layout preview alone still signals loading.

```css
@media (prefers-reduced-motion: reduce) {
  .skeleton::after { animation: none; content: none; }
}
```
