# Particle Field Hero

A reusable React Three Fiber background layer for hero sections: thousands of
drifting points with mouse parallax — one draw call, lazy-loaded so it never
blocks first paint.

## When to use

- Hero/landing sections that need depth without video or heavy WebGL scenes.
- Dark-themed pages where subtle motion adds perceived polish.
- Only when an extra ~120 KB gzipped chunk after hydration is acceptable.

## Architecture

Three pieces:

1. `ParticleField` — the R3F `<Canvas>` plus scene, dynamically imported.
2. A static fallback image shown by default; the canvas fades in over it.
3. A small hook reporting device tier and `prefers-reduced-motion`.

## Implementation sketch

```tsx
// ParticleField.tsx
import { Canvas, useFrame } from '@react-three/fiber'
import * as THREE from 'three'
import { useMemo, useRef } from 'react'

const vertex = /* glsl */ `
  attribute float aScale;
  uniform float uTime;
  uniform vec2 uMouse;
  varying float vAlpha;
  void main() {
    vec3 p = position;
    p.y += sin(uTime * 0.2 + p.x * 0.5) * 0.05;
    p.x += cos(uTime * 0.15 + p.z * 0.4) * 0.03;
    p.xy += uMouse * 0.15 * aScale;
    vec4 mv = modelViewMatrix * vec4(p, 1.0);
    gl_Position = projectionMatrix * mv;
    gl_PointSize = aScale * (12.0 / -mv.z);
    vAlpha = smoothstep(14.0, 4.0, -mv.z);
  }
`

const fragment = /* glsl */ `
  precision mediump float;
  varying float vAlpha;
  void main() {
    float d = length(gl_PointCoord - 0.5);
    if (d > 0.5) discard;
    gl_FragColor = vec4(0.85, 0.9, 1.0, vAlpha * (1.0 - d * 2.0));
  }
`
```

Use `<points>` (single geometry, single material → one draw call) instead of
an `InstancedMesh` here: each particle is just a soft sprite, so instanced
meshes buy nothing. Reach for InstancedMesh only when particles need distinct
geometry or per-instance colors.

```tsx
function Field({ count }: { count: number }) {
  const mat = useRef<THREE.ShaderMaterial>(null!)
  const { positions, scales } = useMemo(() => {
    const positions = new Float32Array(count * 3)
    const scales = new Float32Array(count)
    for (let i = 0; i < count; i++) {
      positions.set(
        [(Math.random() - 0.5) * 20, (Math.random() - 0.5) * 10, -Math.random() * 10],
        i * 3,
      )
      scales[i] = Math.random()
    }
    return { positions, scales }
  }, [count])

  useFrame(({ clock, pointer }) => {
    mat.current.uniforms.uTime.value = clock.elapsedTime
    mat.current.uniforms.uMouse.value.lerp(pointer, 0.04)
  })

  return (
    <points frustumCulled={false}>
      <bufferGeometry>
        <bufferAttribute attach="attributes-position" args={[positions, 3]} />
        <bufferAttribute attach="attributes-aScale" args={[scales, 1]} />
      </bufferGeometry>
      <shaderMaterial
        ref={mat}
        transparent
        depthWrite={false}
        vertexShader={vertex}
        fragmentShader={fragment}
        uniforms={{ uTime: { value: 0 }, uMouse: { value: new THREE.Vector2() } }}
      />
    </points>
  )
}
```

Drift is baked into the vertex shader (no per-frame CPU work). Parallax eases
toward the pointer via `lerp`, so it never snaps. Tune drift amplitude — not
particle count — when the motion feels too strong or too weak.

## Performance budget

| Tier | Particles | Target |
|---|---|---|
| Desktop (dpr capped at 1.75) | ≤ 6 000 | 60 fps, < 3 ms frame time |
| Mobile / low-core (`hardwareConcurrency <= 4`) | ≤ 1 500 | 30 fps |

- Cap `dpr={[1, 1.75]}` on the `<Canvas>`; full-retina point rendering is waste.
- Pause the loop when scrolled away: toggle `frameloop` from an
  IntersectionObserver.
- Verify on a mid-range Android before merging; if frames miss budget, halve
  counts before touching the shader.

## Lazy loading

Never ship this in the critical bundle:

```tsx
const ParticleField = dynamic(() => import('./ParticleField'), {
  ssr: false,
  loading: () => <img src="/hero-fallback.webp" alt="" aria-hidden />,
})
```

Render the static image server-side; the client chunk loads after hydration or
on `requestIdleCallback`. Keep the GLSL as template literals — no extra
bundler loader required.

## Reduced motion

With `prefers-reduced-motion: reduce`, skip mounting the Canvas entirely and
ship the static image. Freezing `uTime` at zero also works, but the static
image is the honest fallback: zero GPU cost, zero battery drain, zero jank.
