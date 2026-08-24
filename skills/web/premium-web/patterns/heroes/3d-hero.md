# Pattern: 3D Hero (Interactive Object)

Real-time 3D hero object (React Three Fiber) with grab-to-spin drag, release
momentum easing back into a slow idle rotation, and a static-image fallback for
touch-only devices.

## When to use

Use when the hero object *is* the story — hardware, footwear, audio gear — and
material and lighting sell what a flat image cannot. Skip for text-heavy pages,
low-end-mobile audiences, or when no modeled asset fits the size budget below.

## Dependencies

```bash
npm install three @react-three/fiber @react-three/drei
```

Asset pipeline: export `.glb` from Blender, compress with `gltfpack -i hero.glb -o hero.min.glb` (Draco/Meshopt), and serve from `/public/models/` with long-lived cache headers.

## Implementation sketch

```tsx
import { Suspense, useEffect, useRef } from "react";
import { Canvas, useFrame } from "@react-three/fiber";
import { useGLTF, ContactShadows } from "@react-three/drei";
import type { Group } from "three";

const MODEL_URL = "/models/hero.min.glb";
const IDLE_SPIN = 0.25; // rad/sec — gate behind prefers-reduced-motion
const SENSITIVITY = 0.005;

function HeroObject() {
  const group = useRef<Group>(null);
  const dragging = useRef(false);
  const lastX = useRef(0);
  const velocity = useRef(IDLE_SPIN);
  const gltf = useGLTF(MODEL_URL);

  useEffect(() => {
    const move = (e: PointerEvent) => {
      if (!dragging.current || !group.current) return;
      const dx = e.clientX - lastX.current;
      lastX.current = e.clientX;
      group.current.rotation.y += dx * SENSITIVITY;
      velocity.current = dx * SENSITIVITY * 60; // rad/sec for release momentum
    };
    const up = () => (dragging.current = false);
    const pairs = [["pointermove", move], ["pointerup", up]] as const;
    pairs.forEach(([name, handler]) => window.addEventListener(name, handler));
    return () => pairs.forEach(([n, h]) => window.removeEventListener(n, h));
  }, []);
  useFrame((_, dt) => {
    const g = group.current;
    if (!g || dragging.current) return;
    velocity.current += (IDLE_SPIN - velocity.current) * Math.min(1, dt * 1.5);
    g.rotation.y += velocity.current * dt;
  });
  return (
    <group ref={group}>
      <primitive
        object={gltf.scene}
        onPointerDown={(e) => {
          e.stopPropagation();
          dragging.current = true;
          lastX.current = e.nativeEvent.clientX;
          velocity.current = 0;
        }}
      />
    </group>
  );
}

export default function Hero3D() {
  return (
    <Canvas camera={{ position: [0, 0.6, 4], fov: 40 }} dpr={[1, 2]}>
      <ambientLight intensity={0.4} />
      <directionalLight position={[3, 4, 5]} intensity={1.2} />
      <Suspense fallback={null}><HeroObject /></Suspense>
      <ContactShadows position={[0, -1.2, 0]} opacity={0.4} blur={2.4} />
    </Canvas>
  );
}

useGLTF.preload(MODEL_URL); // warm the fetch before first render
```

- Listeners attach to `window`, so a drag leaving the canvas doesn't stick — the classic hand-rolled-orbit-controls bug.
- Gate `IDLE_SPIN` behind `prefers-reduced-motion` so the object holds still unless actively dragged.

## Usage

1. Lazy-load `Hero3D` (`next/dynamic` / `React.lazy`) so three.js stays out of the entry bundle; call `useGLTF.preload()` from the layout shell when the hero is first-paint content.
2. Size the container with CSS `aspect-ratio`, never fixed pixel heights.

## Mobile fallback

On narrow viewports or coarse-pointer devices, swap the Canvas for a pre-rendered still captured from the same default camera angle, so both variants read as the same object:

```tsx
const lite = matchMedia("(max-width: 767px)").matches || matchMedia("(pointer: coarse)").matches;
return lite ? (
  <img src="/models/hero-poster.avif" alt="Product at its signature angle" width={1280} height={720} fetchPriority="high" />
) : (
  <Hero3D />
);
```

## Performance notes

| Item           | Budget      | Notes                                  |
| -------------- | ----------- | -------------------------------------- |
| `.glb` payload | ≤ 800 KB    | Draco/Meshopt; 1.5 MB absolute ceiling |
| Textures       | ≤ 1024²     | KTX2/Basis over PNG; ≤ 2 materials     |
| Draw calls     | < 30/frame  | merge geometry, prune orphan nodes     |
| Frame time     | < 8 ms      | headroom inside the 16 ms frame        |

- Toggle `frameloop` to `"never"` via IntersectionObserver when the hero scrolls offscreen; verify drag latency stays under 100 ms under CPU ×4 throttling.
