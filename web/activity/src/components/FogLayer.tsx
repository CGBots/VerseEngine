import { useFrame, useThree } from '@react-three/fiber';
import { useEffect, useMemo } from 'react';
import * as THREE from 'three';
import type { MapView } from '../api';
import { drawFogMask, fogShapes } from '../map/fog';
import { SCENE_SIZE } from '../map/geometry';

/** Taille du masque (pixels du plus grand côté). */
const MASK_SIZE = 512;
/** La brume déborde largement de la carte : les bords de la vue restent dans la brume. */
const OVERSCAN = 3;
/** Ordre de rendu de la brume : routes empruntées, lieux et noms sont dessinés après. */
export const FOG_ORDER = 5;
/** Images par seconde de l'animation (la scène est rendue à la demande). */
const FPS = 30;

const vertex = /* glsl */ `
  varying vec2 vUv;
  void main() {
    vUv = uv;
    gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
  }
`;

/**
 * Brume (Q4) : opaque, animée, dégagée seulement là où le masque est blanc. Le bruit (somme
 * fractale de bruit de valeur) dérive lentement et déforme le bord des zones découvertes.
 */
const fragment = /* glsl */ `
  uniform sampler2D uMask;
  uniform float uTime;
  uniform vec2 uMaskScale;
  varying vec2 vUv;

  float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
  float noise(vec2 p) {
    vec2 i = floor(p), f = fract(p);
    vec2 u = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash(i), hash(i + vec2(1, 0)), u.x), mix(hash(i + vec2(0, 1)), hash(i + vec2(1, 1)), u.x), u.y);
  }
  float fbm(vec2 p) {
    float s = 0.0, a = 0.5;
    for (int o = 0; o < 5; o++) { s += a * noise(p); p *= 2.03; a *= 0.5; }
    return s;
  }

  void main() {
    // UV de la carte dans le plan agrandi.
    vec2 mapUv = (vUv - 0.5) * uMaskScale + 0.5;
    float inside = step(0.0, mapUv.x) * step(mapUv.x, 1.0) * step(0.0, mapUv.y) * step(mapUv.y, 1.0);
    float clear = texture2D(uMask, mapUv).r * inside;
    vec2 p = vUv * 18.0;
    float n1 = fbm(p + vec2(uTime * 0.05, uTime * 0.03));
    float n2 = fbm(p * 2.1 - vec2(uTime * 0.08, -uTime * 0.02));
    // Le bord des zones découvertes ondule avec la brume.
    float open = smoothstep(0.35, 0.75, clear + (n1 - 0.5) * 0.45);
    vec3 dark = vec3(0.56, 0.60, 0.66);
    vec3 light = vec3(0.86, 0.89, 0.93);
    vec3 color = mix(dark, light, smoothstep(0.25, 0.85, n2 * 0.6 + n1 * 0.6));
    gl_FragColor = vec4(color, 1.0 - open);
  }
`;

/** Couche de brume de la scène 3D, au-dessus du relief. Rien si la carte n'a pas de brouillard. */
export function FogLayer({ map, height }: { map: MapView; height: number }) {
  const { invalidate } = useThree();
  const shapes = useMemo(() => fogShapes(map), [map]);
  const side = Math.max(map.width, map.height) || 1;
  const sceneW = (map.width / side) * SCENE_SIZE;
  const sceneH = (map.height / side) * SCENE_SIZE;

  const texture = useMemo(() => {
    if (!shapes) return null;
    const canvas = document.createElement('canvas');
    const scale = MASK_SIZE / side;
    canvas.width = Math.max(2, Math.round(map.width * scale));
    canvas.height = Math.max(2, Math.round(map.height * scale));
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    // Bords adoucis (ignoré par les navigateurs sans `filter` : les dégradés suffisent).
    ctx.filter = 'blur(6px)';
    drawFogMask(ctx, shapes, scale, canvas.width, canvas.height);
    const tex = new THREE.CanvasTexture(canvas);
    tex.colorSpace = THREE.NoColorSpace;
    return tex;
  }, [shapes, map.width, map.height, side]);
  useEffect(() => () => texture?.dispose(), [texture]);

  const material = useMemo(
    () =>
      texture &&
      new THREE.ShaderMaterial({
        uniforms: { uMask: { value: texture }, uTime: { value: 0 }, uMaskScale: { value: new THREE.Vector2(OVERSCAN * SCENE_SIZE / sceneW, OVERSCAN * SCENE_SIZE / sceneH) } },
        vertexShader: vertex,
        fragmentShader: fragment,
        transparent: true,
        depthWrite: false,
      }),
    [texture, sceneW, sceneH],
  );
  useEffect(() => () => material?.dispose(), [material]);

  // Animation : la scène n'est rendue qu'à la demande, on la réveille à FPS images par seconde.
  useEffect(() => {
    if (!material) return;
    const id = window.setInterval(() => invalidate(), 1000 / FPS);
    return () => window.clearInterval(id);
  }, [material, invalidate]);
  useFrame((state) => {
    if (material) material.uniforms.uTime.value = state.clock.elapsedTime;
  });

  if (!material) return null;
  return (
    <mesh position={[0, height, 0]} rotation={[-Math.PI / 2, 0, 0]} material={material} renderOrder={FOG_ORDER}>
      <planeGeometry args={[OVERSCAN * SCENE_SIZE, OVERSCAN * SCENE_SIZE]} />
    </mesh>
  );
}
