import { Canvas, useThree } from '@react-three/fiber';
import { useEffect, useMemo } from 'react';
import * as THREE from 'three';
import { MapControls } from 'three/examples/jsm/controls/MapControls.js';
import type { MapView } from '../api';
import { SCENE_SIZE, terrainHeight, toScene, type ScenePoint } from '../map/geometry';

const RELIEF = 6; // hauteur maximale du relief (unités de scène)

interface Props {
  map: MapView;
  currentPlaceId: string | null;
  reachable: Set<string>;
  player: ScenePoint | null;
  onSelectPlace: (id: string) => void;
}

/** Contrôles de carte (glisser = déplacer, molette = zoom). Rendu à la demande uniquement. */
function Controls() {
  const { camera, gl, invalidate } = useThree();
  useEffect(() => {
    const controls = new MapControls(camera, gl.domElement);
    controls.enableDamping = false;
    controls.maxPolarAngle = Math.PI / 2.4;
    controls.minDistance = 10;
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

function Terrain({ map }: { map: MapView }) {
  const geometry = useMemo(() => {
    const t = map.terrain;
    const side = Math.max(map.width, map.height) || 1;
    const w = (map.width / side) * SCENE_SIZE;
    const h = (map.height / side) * SCENE_SIZE;
    const cols = t ? Math.max(1, t.cols - 1) : 1;
    const rows = t ? Math.max(1, t.rows - 1) : 1;
    const g = new THREE.PlaneGeometry(w, h, cols, rows);
    g.rotateX(-Math.PI / 2);
    if (t) {
      const pos = g.attributes.position as THREE.BufferAttribute;
      const colors: number[] = [];
      for (let i = 0; i < pos.count; i++) {
        const v = (t.heights[i] ?? 0) / 255;
        // Azgaar : < 20/100 = eau, laissée à plat.
        const land = v >= 0.2;
        pos.setY(i, land ? (v - 0.2) * RELIEF : 0);
        const c = land ? new THREE.Color().setHSL(0.28 - v * 0.2, 0.35, 0.25 + v * 0.35) : new THREE.Color('#2b4a6b');
        colors.push(c.r, c.g, c.b);
      }
      g.setAttribute('color', new THREE.Float32BufferAttribute(colors, 3));
      g.computeVertexNormals();
    }
    return g;
  }, [map]);
  useEffect(() => () => geometry.dispose(), [geometry]);
  return (
    <mesh geometry={geometry}>
      <meshLambertMaterial vertexColors={!!map.terrain} color={map.terrain ? undefined : '#33363d'} />
    </mesh>
  );
}

function Roads({ map }: { map: MapView }) {
  const lines = useMemo(() => {
    const byId = new Map(map.places.map((p) => [p.id, p]));
    return map.roads.flatMap((r) => {
      const a = byId.get(r.from);
      const b = byId.get(r.to);
      if (!a || !b) return [];
      const pa = toScene(a.x, a.y, map.width, map.height);
      const pb = toScene(b.x, b.y, map.width, map.height);
      const ya = terrainHeight(map, a.x, a.y) * RELIEF + 0.3;
      const yb = terrainHeight(map, b.x, b.y) * RELIEF + 0.3;
      const g = new THREE.BufferGeometry().setFromPoints([new THREE.Vector3(pa.x, ya, pa.z), new THREE.Vector3(pb.x, yb, pb.z)]);
      return [{ id: r.id, geometry: g, secret: r.secret }];
    });
  }, [map]);
  useEffect(() => () => lines.forEach((l) => l.geometry.dispose()), [lines]);
  return (
    <>
      {lines.map((l) => (
        <lineSegments key={l.id} geometry={l.geometry}>
          <lineBasicMaterial color={l.secret ? '#c9a227' : '#d9d9d9'} />
        </lineSegments>
      ))}
    </>
  );
}

export function MapScene({ map, currentPlaceId, reachable, player, onSelectPlace }: Props) {
  return (
    <Canvas
      frameloop="demand"
      dpr={[1, 1.5]}
      camera={{ position: [0, SCENE_SIZE * 0.7, SCENE_SIZE * 0.6], fov: 45, near: 0.5, far: SCENE_SIZE * 5 }}
      gl={{ antialias: true, powerPreference: 'low-power' }}
    >
      <color attach="background" args={['#1e1f22']} />
      <hemisphereLight args={['#ffffff', '#444444', 1.6]} />
      <directionalLight position={[30, 60, 20]} intensity={0.8} />
      <Controls />
      <Terrain map={map} />
      <Roads map={map} />
      {map.places.map((p) => {
        const s = toScene(p.x, p.y, map.width, map.height);
        const y = (map.terrain ? Math.max(0, terrainHeight(map, p.x, p.y) - 0.2) * RELIEF : 0) + 0.8;
        const color = p.id === currentPlaceId ? '#57f287' : reachable.has(p.id) ? '#5865f2' : '#b5bac1';
        return (
          <mesh
            key={p.id}
            position={[s.x, y, s.z]}
            onClick={(e) => {
              e.stopPropagation();
              onSelectPlace(p.id);
            }}
          >
            <cylinderGeometry args={[0.9, 0.9, 1.6, 16]} />
            <meshLambertMaterial color={color} />
          </mesh>
        );
      })}
      {player && (
        <mesh position={[player.x, 2.4, player.z]}>
          <sphereGeometry args={[0.9, 16, 12]} />
          <meshLambertMaterial color="#fee75c" emissive="#6b5a00" />
        </mesh>
      )}
    </Canvas>
  );
}
