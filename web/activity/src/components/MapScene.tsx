import { Canvas, useThree } from '@react-three/fiber';
import { useEffect, useMemo } from 'react';
import * as THREE from 'three';
import { MapControls } from 'three/examples/jsm/controls/MapControls.js';
import type { MapView } from '../api';
import { SCENE_SIZE, toScene, type PlayerMotion } from '../map/geometry';
import { useAnimatedPosition } from '../map/useMotion';
import { biomeColor, LAND, proceduralTerrain, type Heightmap } from '../map/procedural';
import { labelTexture, PIN_ASPECT, pinTexture } from '../map/sprites';

const RELIEF = 7; // hauteur maximale du relief (unités de scène)

interface Props {
  map: MapView;
  currentPlaceId: string | null;
  reachable: Set<string>;
  motion: PlayerMotion | null;
  playerPicture: string | null;
  playerName: string;
  onSelectPlace: (id: string) => void;
}

/** Relief de la carte : heightmap Azgaar si présente, sinon terrain généré. */
function useHeightmap(map: MapView): Heightmap {
  return useMemo(() => {
    if (map.terrain && map.terrain.cols > 1 && map.terrain.rows > 1) return map.terrain;
    const cols = 180;
    const rows = Math.max(2, Math.round((cols * map.height) / Math.max(1, map.width)));
    return proceduralTerrain(map, cols, rows);
  }, [map]);
}

/** Hauteur (unités de scène) au point de carte (x, y), interpolée bilinéairement. */
function elevation(hm: Heightmap, map: MapView, x: number, y: number): number {
  const fx = Math.min(hm.cols - 1, Math.max(0, (x / map.width) * hm.cols - 0.5));
  const fy = Math.min(hm.rows - 1, Math.max(0, (y / map.height) * hm.rows - 0.5));
  const x0 = Math.floor(fx), y0 = Math.floor(fy);
  const x1 = Math.min(hm.cols - 1, x0 + 1), y1 = Math.min(hm.rows - 1, y0 + 1);
  const h = (c: number, r: number) => (hm.heights[r * hm.cols + c] ?? 0) / 255;
  const tx = fx - x0, ty = fy - y0;
  const v = (h(x0, y0) * (1 - tx) + h(x1, y0) * tx) * (1 - ty) + (h(x0, y1) * (1 - tx) + h(x1, y1) * tx) * ty;
  return v >= LAND ? (v - LAND) * RELIEF : 0;
}

/** Contrôles de carte (glisser = déplacer, molette/pincer = zoom). Rendu à la demande. */
function Controls() {
  const { camera, gl, invalidate } = useThree();
  useEffect(() => {
    const controls = new MapControls(camera, gl.domElement);
    controls.enableDamping = false;
    controls.maxPolarAngle = Math.PI / 2.5;
    controls.minDistance = 8;
    controls.maxDistance = SCENE_SIZE * 1.6;
    const onChange = () => invalidate();
    controls.addEventListener('change', onChange);
    return () => {
      controls.removeEventListener('change', onChange);
      controls.dispose();
    };
  }, [camera, gl, invalidate]);
  return null;
}

function Terrain({ map, hm }: { map: MapView; hm: Heightmap }) {
  const geometry = useMemo(() => {
    const side = Math.max(map.width, map.height) || 1;
    const g = new THREE.PlaneGeometry((map.width / side) * SCENE_SIZE, (map.height / side) * SCENE_SIZE, hm.cols - 1, hm.rows - 1);
    g.rotateX(-Math.PI / 2);
    const pos = g.attributes.position as THREE.BufferAttribute;
    const colors: number[] = [];
    for (let i = 0; i < pos.count; i++) {
      const v = (hm.heights[i] ?? 0) / 255;
      pos.setY(i, v >= LAND ? (v - LAND) * RELIEF : 0);
      const [r, gr, b] = biomeColor(v);
      colors.push(r / 255, gr / 255, b / 255);
    }
    g.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));
    g.computeVertexNormals();
    return g;
  }, [map, hm]);
  useEffect(() => () => geometry.dispose(), [geometry]);
  return (
    <mesh geometry={geometry}>
      <meshLambertMaterial vertexColors />
    </mesh>
  );
}

/** Routes : rubans qui suivent le relief (échantillonnés le long du segment). */
function Roads({ map, hm }: { map: MapView; hm: Heightmap }) {
  const tubes = useMemo(() => {
    const byId = new Map(map.places.map((p) => [p.id, p]));
    return map.roads.flatMap((r) => {
      const a = byId.get(r.from);
      const b = byId.get(r.to);
      if (!a || !b) return [];
      const steps = 24;
      const pts: THREE.Vector3[] = [];
      for (let i = 0; i <= steps; i++) {
        const t = i / steps;
        const x = a.x + (b.x - a.x) * t;
        const y = a.y + (b.y - a.y) * t;
        const s = toScene(x, y, map.width, map.height);
        pts.push(new THREE.Vector3(s.x, elevation(hm, map, x, y) + 0.25, s.z));
      }
      const geometry = new THREE.TubeGeometry(new THREE.CatmullRomCurve3(pts), steps * 2, 0.22, 6, false);
      return [{ id: r.id, geometry, secret: r.secret }];
    });
  }, [map, hm]);
  useEffect(() => () => tubes.forEach((t) => t.geometry.dispose()), [tubes]);
  return (
    <>
      {tubes.map((t) => (
        <mesh key={t.id} geometry={t.geometry}>
          <meshLambertMaterial color={t.secret ? '#d4a72c' : '#efe6d2'} />
        </mesh>
      ))}
    </>
  );
}

/** Lieu : cercle à plat (pastille + contour) et son nom. */
function Place({ x, y, z, name, color, onClick }: { x: number; y: number; z: number; name: string; color: string; onClick: () => void }) {
  const label = useMemo(() => labelTexture(name), [name]);
  useEffect(() => () => label.texture.dispose(), [label]);
  return (
    <group position={[x, y, z]}>
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, 0.06, 0]} onClick={(e) => { e.stopPropagation(); onClick(); }}>
        <circleGeometry args={[1.25, 32]} />
        <meshBasicMaterial color={color} />
      </mesh>
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, 0.08, 0]}>
        <ringGeometry args={[1.25, 1.6, 32]} />
        <meshBasicMaterial color="#141518" />
      </mesh>
      <sprite position={[0, 2.4, 0]} scale={[0.035 * label.aspect, 0.035, 1]}>
        <spriteMaterial map={label.texture} sizeAttenuation={false} depthTest={false} transparent />
      </sprite>
    </group>
  );
}

/** Épingle du joueur, ancrée par sa pointe, de taille constante à l'écran. */
function PlayerPin({ x, y, z, picture, name }: { x: number; y: number; z: number; picture: string | null; name: string }) {
  const { invalidate } = useThree();
  const texture = useMemo(() => pinTexture(picture, name, () => invalidate()), [picture, name, invalidate]);
  useEffect(() => () => texture.dispose(), [texture]);
  const h = 0.11;
  return (
    <sprite position={[x, y, z]} center={new THREE.Vector2(0.5, 0)} scale={[h * PIN_ASPECT, h, 1]} renderOrder={10}>
      <spriteMaterial map={texture} sizeAttenuation={false} depthTest={false} transparent />
    </sprite>
  );
}

/** Tube qui suit le relief entre deux points de carte. */
function terrainTube(map: MapView, hm: Heightmap, a: { x: number; y: number }, b: { x: number; y: number }, radius: number, lift: number, steps = 48) {
  const pts: THREE.Vector3[] = [];
  for (let i = 0; i <= steps; i++) {
    const t = i / steps;
    const x = a.x + (b.x - a.x) * t;
    const y = a.y + (b.y - a.y) * t;
    const s = toScene(x, y, map.width, map.height);
    pts.push(new THREE.Vector3(s.x, elevation(hm, map, x, y) + lift, s.z));
  }
  return new THREE.TubeGeometry(new THREE.CatmullRomCurve3(pts), steps * 2, radius, 8, false);
}

const RADIAL = 8;

/** Joueur : épingle animée en continu et partie parcourue du trajet. */
function Player({ map, hm, motion, picture, name }: { map: MapView; hm: Heightmap; motion: PlayerMotion; picture: string | null; name: string }) {
  const animated = useAnimatedPosition(motion);
  const traveled = useMemo(
    () => (motion.kind === 'road' ? terrainTube(map, hm, motion.from, motion.to, 0.34, 0.3) : null),
    [map, hm, motion],
  );
  useEffect(() => () => traveled?.dispose(), [traveled]);
  if (!animated) return null;
  if (traveled && animated.progress != null) {
    // 6 indices par face, RADIAL faces par anneau, 2 × 48 anneaux le long du tube.
    const rings = Math.round(animated.progress * 96);
    traveled.setDrawRange(0, rings * RADIAL * 6);
  }
  const s = toScene(animated.at.x, animated.at.y, map.width, map.height);
  return (
    <>
      {traveled && (
        <mesh geometry={traveled}>
          <meshBasicMaterial color="#5865f2" />
        </mesh>
      )}
      <PlayerPin x={s.x} y={elevation(hm, map, animated.at.x, animated.at.y) + 0.3} z={s.z} picture={picture} name={name} />
    </>
  );
}

export function MapScene({ map, currentPlaceId, reachable, motion, playerPicture, playerName, onSelectPlace }: Props) {
  const hm = useHeightmap(map);
  return (
    <Canvas
      frameloop="demand"
      dpr={[1, 1.5]}
      camera={{ position: [0, SCENE_SIZE * 0.62, SCENE_SIZE * 0.55], fov: 45, near: 0.5, far: SCENE_SIZE * 5 }}
      gl={{ antialias: true, powerPreference: 'low-power' }}
    >
      <color attach="background" args={['#16324f']} />
      <fog attach="fog" args={['#16324f', SCENE_SIZE * 1.1, SCENE_SIZE * 2.6]} />
      <hemisphereLight args={['#fff8ec', '#3a4250', 1.25]} />
      <directionalLight position={[-40, 55, 25]} intensity={1.1} />
      <Controls />
      <Terrain map={map} hm={hm} />
      <Roads map={map} hm={hm} />
      {map.places.map((p) => {
        const s = toScene(p.x, p.y, map.width, map.height);
        const color = p.id === currentPlaceId ? '#57f287' : reachable.has(p.id) ? '#5865f2' : '#f2f3f5';
        return <Place key={p.id} x={s.x} y={elevation(hm, map, p.x, p.y)} z={s.z} name={p.name} color={color} onClick={() => onSelectPlace(p.id)} />;
      })}
      {motion && <Player map={map} hm={hm} motion={motion} picture={playerPicture} name={playerName} />}
    </Canvas>
  );
}
