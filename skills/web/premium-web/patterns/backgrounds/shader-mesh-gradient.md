# Pattern: Animated Mesh Gradient Shader Background

A fluid, organic "mesh gradient" background — several colors blended through
2D simplex noise so the blobs drift and morph continuously. Rendered on the GPU
with a fragment shader over a single fullscreen quad. Used for hero sections,
login screens, and premium landing pages where a static gradient feels flat.

## When to use

- Hero/landing backgrounds that need motion without video cost.
- Ambient backgrounds behind frosted-glass cards.
- Anywhere you would otherwise ship a large looping `.webm` of a gradient.

## How it works

1. A fullscreen quad (or canvas) is drawn once per frame.
2. Each pixel computes 3 simplex noise fields at slowly increasing time offsets.
3. Each noise value becomes the weight of one gradient color; weights are
   normalized and blended, producing soft moving color regions ("mesh").
4. A subtle vignette keeps edges calm so overlaid UI stays legible.

## GLSL fragment shader

```glsl
precision highp float;

uniform vec2  uResolution;
uniform float uTime;
uniform vec3  uColorA; // e.g. #0f0c29 -> vec3(0.059, 0.047, 0.161)
uniform vec3  uColorB; // e.g. #302b63
uniform vec3  uColorC; // e.g. #24243e

// Ashima Arts 2D simplex noise (MIT) — trimmed for brevity
vec3 mod289(vec3 x){ return x - floor(x * (1.0/289.0)) * 289.0; }
vec2 mod289(vec2 x){ return x - floor(x * (1.0/289.0)) * 289.0; }
vec3 permute(vec3 x){ return mod289(((x*34.0)+1.0)*x); }

float snoise(vec2 v){
    const vec4 C = vec4(0.211324865405187, 0.366025403784439,
                       -0.577350269189626, 0.024390243902439);
    vec2 i  = floor(v + dot(v, C.yy));
    vec2 x0 = v - i + dot(i, C.xx);
    vec2 i1 = (x0.x > x0.y) ? vec2(1.0, 0.0) : vec2(0.0, 1.0);
    vec4 x12 = x0.xyxy + C.xxzz;
    x12.xy -= i1;
    i = mod289(i);
    vec3 p = permute(permute(i.y + vec3(0.0, i1.y, 1.0))
                   + i.x + vec3(0.0, i1.x, 1.0));
    vec3 m = max(0.5 - vec3(dot(x0,x0), dot(x12.xy,x12.xy),
                            dot(x12.zw,x12.zw)), 0.0);
    m = m*m; m = m*m;
    vec3 x = 2.0 * fract(p * C.www) - 1.0;
    vec3 h = abs(x) - 0.5;
    vec3 ox = floor(x + 0.5);
    vec3 a0 = x - ox;
    m *= 1.79284291400159 - 0.85373472095314 * (a0*a0 + h*h);
    vec3 g;
    g.x = a0.x * x0.x + h.x * x0.y;
    g.yz = a0.yz * x12.xz + h.yz * x12.yw;
    return 130.0 * dot(m, g);
}

void main(){
    vec2 uv = gl_FragCoord.xy / uResolution.xy;
    float t = uTime * 0.08;

    // Slowly drifting noise "blobs"
    float n1 = snoise(uv * 1.6 + vec2(t,        t * 0.6));
    float n2 = snoise(uv * 2.3 - vec2(t * 0.7,  t * 0.4) + 10.0);
    float n3 = snoise(uv * 1.1 + vec2(t * 0.3, -t * 0.8) + 20.0);

    // Map noise (-1..1) to 0..1 weights
    float wB = smoothstep(-0.6, 0.6, n1);
    float wC = smoothstep(-0.5, 0.7, n2) * (1.0 - wB);

    vec3 col = mix(uColorA, uColorB, wB);
    col      = mix(col,     uColorC, wC);

    // Vignette: darken corners slightly
    float vig = smoothstep(1.25, 0.35, length(uv - 0.5));
    col *= mix(0.75, 1.0, vig);

    gl_FragColor = vec4(col, 1.0);
}
```

## Usage A — Three.js fullscreen plane

```js
import * as THREE from "three";

const renderer = new THREE.WebGLRenderer({
  canvas: document.querySelector("#bg"),
  antialias: false,
  powerPreference: "low-power",
});
renderer.setPixelRatio(Math.min(window.devicePixelRatio, 1.5)); // DPR cap!

const scene = new THREE.Scene();
const camera = new THREE.OrthographicCamera(-1, 1, 1, -1, 0, 1);

const uniforms = {
  uTime:       { value: 0 },
  uResolution: { value: new THREE.Vector2() },
  uColorA:     { value: new THREE.Color("#0f0c29") },
  uColorB:     { value: new THREE.Color("#302b63") },
  uColorC:     { value: new THREE.Color("#24243e") },
};

scene.add(new THREE.Mesh(
  new THREE.PlaneGeometry(2, 2),
  new THREE.ShaderMaterial({ uniforms, fragmentShader: FRAG, depthTest: false })
));

function resize() {
  renderer.setSize(innerWidth, innerHeight);
  uniforms.uResolution.value.set(innerWidth, innerHeight);
}
addEventListener("resize", resize); resize();

renderer.setAnimationLoop((tMs) => {
  uniforms.uTime.value = tMs / 1000;
  renderer.render(scene, camera);
});
```

## Usage B — Raw WebGL canvas

For zero-dependency pages: compile the shader, bind `uResolution`/`uTime`,
and draw a fullscreen triangle (`gl.drawArrays(gl.TRIANGLES, 0, 3)` with no
vertex buffer, using `gl_VertexID` positions or a static attribute). Same
fragment shader applies verbatim.

## Fallback for low-power devices

- Detect once at load:
  ```js
  const weak =
    matchMedia("(prefers-reduced-motion: reduce)").matches ||
    navigator.hardwareConcurrency <= 2 ||
    !document.createElement("canvas").getContext("webgl");
  ```
- If weak: hide the canvas and set the container background to a **static
  CSS mesh gradient** (pre-rendered image or layered radial-gradients):
  ```css
  .mesh-fallback {
    background:
      radial-gradient(at 20% 30%, #302b63 0, transparent 50%),
      radial-gradient(at 80% 70%, #24243e 0, transparent 55%),
      #0f0c29;
  }
  ```
- Also pause rendering when `document.hidden === true` (visibilitychange).

## Performance notes

- **Cap DPR** at ~1.5 (never raw `devicePixelRatio`, which hits 3+ on phones).
  Gradient noise has no fine detail; upscaling artifacts are invisible.
- Optionally render internally at 0.5–0.75× resolution and let CSS upscale.
- Keep noise scale low (~1–2.5 octaves here, no fBm stacking). Each extra
  octave doubles ALU per pixel.
- `antialias: false` — irrelevant for a fullscreen gradient.
- Throttle to 30fps if profiling shows sustained GPU load: accumulate time and
  skip alternate frames instead of calling `requestAnimationFrame` twice.
- One draw call, three uniforms/frame → CPU cost is negligible; all cost is
  fragment-shading, which scales with pixels × DPR².
