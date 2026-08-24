# Product Showcase (Interactive 3D)

Reusable drag-to-rotate product viewer for React Three Fiber: a GLTF model visitors
orbit freely, hotspot annotations revealed on hover/focus, and variant/color swaps
applied through material changes — with an image-carousel fallback on mobile.

## When to use

- PDPs and landing pages where shape and material sell the product (footwear, audio gear).
- Variant selection that should feel tactile instead of a static thumbnail strip.
- Skip it for text-heavy SKUs where a photo grid beats live 3D.

## Dependencies

Install `three @react-three/fiber @react-three/drei` (plus `@types/three` as a dev dep).

Asset contract: one compressed `.glb` (DRACO geometry, KTX2 textures), origin centered under the product, Y-up, real-world scale.

## Implementation sketch

```tsx
import { Suspense, useEffect, useRef, useState } from "react";
import { Canvas, useFrame } from "@react-three/fiber";
import { Html, OrbitControls, useGLTF } from "@react-three/drei";
import * as THREE from "three";

type Variant = { id: string; label: string; hex: string };
type Hotspot = { pos: [number, number, number]; title: string; body: string };

function Model({ url, hex, spots }: { url: string; hex: string; spots: Hotspot[] }) {
  const g = useRef<THREE.Group>(null!);
  const dragging = useRef(false);
  const [open, setOpen] = useState<string | null>(null);
  const { scene } = useGLTF(url);

  // Variant swap: one shared material retints every mesh in the file.
  useEffect(() => {
    const mat = new THREE.MeshStandardMaterial({ color: hex, roughness: 0.35 });
    scene.traverse((o) => { if ((o as THREE.Mesh).isMesh) (o as THREE.Mesh).material = mat; });
    return () => mat.dispose();
  }, [hex, scene]);

  // Idle spin, paused while the visitor drags.
  useFrame((_, dt) => {
    if (!dragging.current) g.current.rotation.y += dt * 0.25;
  });

  return (
    <group ref={g} onPointerDown={() => (dragging.current = true)} onPointerUp={() => (dragging.current = false)}>
      <primitive object={scene} />
      {spots.map((s) => (
        <Html key={s.title} position={s.pos} center>
          <button className="dot" aria-label={s.title} onMouseEnter={() => setOpen(s.title)}
            onMouseLeave={() => setOpen(null)} onFocus={() => setOpen(s.title)} onBlur={() => setOpen(null)} />
          {open === s.title && <div className="tip"><strong>{s.title}</strong><p>{s.body}</p></div>}
        </Html>
      ))}
    </group>
  );
}

export function ProductShowcase({ url, variants, spots }: {
  url: string; variants: Variant[]; spots: Hotspot[];
}) {
  const [variant, setVariant] = useState(variants[0]);
  return (
    <section className="showcase">
      <Canvas camera={{ position: [0, 0.5, 4] }} dpr={[1, 1.75]}>
        <ambientLight intensity={0.8} />
        <directionalLight position={[3, 4, 5]} intensity={1.1} />
        <Suspense fallback={null}>
          <Model url={url} hex={variant.hex} spots={spots} />
        </Suspense>
        <OrbitControls enablePan={false} enableZoom={false} />
      </Canvas>
      <nav role="radiogroup" aria-label="Color variants">
        {variants.map((v) => (
          <button key={v.id} role="radio" aria-checked={v.id === variant.id}
            style={{ background: v.hex }} onClick={() => setVariant(v)}>{v.label}</button>
        ))}
      </nav>
    </section>
  );
}
```

Horizontal drag orbits yaw, vertical drag tilts inside OrbitControls' polar clamps.
Hotspots live in model space against the exported GLTF, surviving every variant swap.

## Usage

```tsx
<ProductShowcase
  url="/models/sneaker.glb"
  variants={[
    { id: "bone", label: "Bone", hex: "#e8e2d6" }, { id: "ember", label: "Ember", hex: "#c2452d" },
  ]}
  spots={[{ pos: [0, 0.9, 0.4], title: "Knit collar", body: "Zero-seam recycled knit." }]}
/>
```

Call `useGLTF.preload("/models/sneaker.glb")` after first paint to warm drei's cache.

## Mobile fallback (image carousel instead of live 3D)

Render the carousel whenever WebGL is unavailable, `(pointer: coarse)` matches, or the device is memory-constrained:

```tsx
const allow3D = window.matchMedia("(pointer: fine)").matches;
return allow3D ? <ProductShowcase {...props} /> : <ShotCarousel frames={turntable[variantId]} />;
```

Ship ~24 offline-rendered turntable frames per variant from the same GLTF so the fallback
matches the 3D view; keep both paths behind the same variant state for identical behavior.

## Performance notes

- Compress hard: DRACO/meshopt geometry + KTX2 textures turns a 40 MB export into a ~2 MB glb.
- Clamp DPR (`dpr={[1, 1.75]}`) instead of rendering at native 3x phone density.
- No runtime shadow maps; bake ambient occlusion into textures.
- One shared material per variant keeps state changes flat regardless of mesh count.
- Lazy-load the Canvas subtree — three.js core (~150 KB gz) must not block first render.
- Gate the idle spin on `document.hidden`, or use drei's `<AdaptiveDpr>` under load.
