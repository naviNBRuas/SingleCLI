# GLSL Shaders & Procedural Effects for Premium Websites

Shaders run on the GPU and unlock visual work that CSS and canvas cannot do:
per-pixel lighting, fluid distortion, volumetric glow, infinite procedural
textures. On premium marketing sites they are usually delivered through WebGL
(raw or via Three.js/OGL) on a full-screen quad or a mesh behind/above DOM
content.

## Vertex vs Fragment Shaders

Every WebGL program pairs two shaders written in GLSL:

- **Vertex shader** — runs once per vertex. Its job is position: read
  attributes (position, uv), apply matrices, output `gl_Position` and pass
  varyings to the fragment stage.
- **Fragment shader** — runs once per *pixel* it covers. Its job is color:
  compute the final RGBA for each pixel. This is where all procedural effects
  live, and where almost all the cost lives too.

```glsl
// Vertex shader: full-screen quad passthrough
attribute vec2 aPosition;
varying vec2 vUv;
void main() {
  vUv = aPosition * 0.5 + 0.5;
  gl_Position = vec4(aPosition, 0.0, 1.0);
}
```

```glsl
// Fragment shader: solid animated color
precision highp float;
uniform float uTime;
varying vec2 vUv;
void main() {
  vec3 color = mix(vec3(0.05), vec3(0.9), vUv.y);
  gl_FragColor = vec4(color, 1.0);
}
```

Uniforms are per-draw constants (`uTime`, resolution, mouse); varyings are
interpolated from vertex to fragment. Keep data flowing vertex → varying →
fragment rather than recomputing per-pixel when possible.

## Noise Functions

Procedural backgrounds live and die by noise. Two staples:

### Hash-based value noise (cheap baseline)

```glsl
float hash(vec2 p) {
  return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123);
}

float noise(vec2 p) {
  vec2 i = floor(p);
  vec2 f = fract(p);
  vec2 u = f * f * (3.0 - 2.0 * f); // smoothstep fade
  return mix(
    mix(hash(i), hash(i + vec2(1.0, 0.0)), u.x),
    mix(hash(i + vec2(0.0, 1.0)), hash(i + vec2(1.0, 1.0)), u.x),
    u.y
  );
}
```

### Classic 2D Perlin-style gradient noise

```glsl
vec2 grad(vec2 p) {
  float h = fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453123) * 6.28318;
  return vec2(cos(h), sin(h));
}

float perlin(vec2 p) {
  vec2 i = floor(p), f = fract(p);
  vec2 u = f * f * f * (f * (f * 6.0 - 15.0) + 10.0);
  float a = dot(grad(i),                 f);
  float b = dot(grad(i + vec2(1, 0)),    f - vec2(1, 0));
  float c = dot(grad(i + vec2(0, 1)),    f - vec2(0, 1));
  float d = dot(grad(i + vec2(1, 1)),    f - vec2(1, 1));
  return mix(mix(a, b, u.x), mix(c, d, u.x), u.y); // range ~[-1, 1]
}
```

For production quality use Ashima's simplex noise (`snoise`) — better
isotropy, fewer visible grid artifacts. Stack octaves into fractal Brownian
motion for richness:

```glsl
float fbm(vec2 p) {
  float v = 0.0, amp = 0.5;
  for (int i = 0; i < 5; i++) {
    v += amp * snoise(p);
    p *= 2.02;
    amp *= 0.5;
  }
  return v;
}
```

## Common Premium Effects

### Animated gradient background

```glsl
void main() {
  vec2 uv = vUv;
  float n = fbm(uv * 2.5 + vec2(uTime * 0.06, -uTime * 0.03));
  vec3 top = vec3(0.04, 0.05, 0.10);
  vec3 bot = vec3(0.35, 0.12, 0.55);
  vec3 col = mix(top, bot, smoothstep(-0.4, 0.6, n + uv.y));
  col += 0.08 * smoothstep(0.55, 0.95, fbm(uv * 4.0 + n)); // subtle sheen
  gl_FragColor = vec4(col, 1.0);
}
```

### Distortion (hover ripple / image displacement)

Displace UVs with noise before sampling a texture — the core trick behind
premium hover transitions:

```glsl
uniform sampler2D uTexture;
uniform float uHover; // 0..1 animated on hover

void main() {
  float d = snoise(vUv * 4.0 + uTime * 0.2) * uHover * 0.15;
  vec2 distortedUv = vUv + vec2(d, d * 0.5);
  vec3 col = texture2D(uTexture, distortedUv).rgb;
  gl_FragColor = vec4(col, 1.0);
}
```

Animate `uHover` with an eased JS tween (GSAP/lerp) so the GPU sees a single
uniform changing — never rebuild shaders per frame.

### Glow (radial falloff, additive)

```glsl
void main() {
  float dist = distance(vUv, vec2(0.5)) * 2.0;
  float glow = exp(-dist * 3.5);            // soft exponential falloff
  vec3 tint = vec3(0.45, 0.30, 1.00);
  vec3 base = vec3(0.02);
  gl_FragColor = vec4(base + glow * tint, 1.0);
}
```

Layer two moving glows over an fbm gradient for the classic "aurora hero".

## Performance Guidance

Fragment cost dominates. A full-screen fragment shader executes once per pixel
per frame — at 1440p that is ~3.7M invocations × 60fps. Budget accordingly:

- **Resolution scaling**: render at `min(devicePixelRatio, 1.5)` or even 1.0×,
  upscale via CSS. This is the single biggest win on retina phones.
- **Octave discipline**: each fbm octave multiplies cost. 3–4 octaves reads as
  "expensive"; 8+ is for offline renders, not websites.
- **Kill branches where possible**: GPUs execute both sides of divergent
  branches. Prefer `mix()` and arithmetic over `if` in hot loops.
- **Mobile GPU limits**: tile-based renderers (Apple/Mali/Adreno) suffer from
  heavy overdraw and large textures. Avoid fullscreen transparent layers
  stacking multiple shader canvases; one composited canvas beats three.
- **Precision**: use `mediump` on mobile fragments unless you truly need
  `highp` (time-driven animation drifts badly at low precision after minutes).
- **Pause when hidden**: stop the RAF loop on `visibilitychange` and
  `IntersectionObserver` exit; a shader offscreen should cost zero.
- **Prefer uniform tweens** over recompiling programs; compile once, cache.
- **Test on low-end Android**, not just M-series MacBooks — that is where
  thermal throttling and dropped frames appear.

Rule of thumb: if the effect can be faked convincingly with CSS gradients +
blend modes, do that; reserve GLSL for what genuinely needs per-pixel math.
