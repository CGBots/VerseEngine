import { Canvas, useThree } from '@react-three/fiber';
import { useEffect, useMemo, useRef } from 'react';
import * as THREE from 'three';
import { MapControls } from 'three/examples/jsm/controls/MapControls.js';
import type { MapView } from '../api';
import { fitDistance, viewElevation, markerScale, placesFrame, pointAlong, roadPath, SCENE_SIZE, toScene, type MapPoint, type PlayerMotion, type SceneFrame } from '../map/geometry';
import { flattenUnderFog, fogFocus, fogShapes } from '../map/fog';
import { useAnimatedPosition } from '../map/useMotion';
import { biomeColor, LAND, proceduralTerrain, type Heightmap } from '../map/procedural';
import { labelTexture, PIN_ASPECT, pinTexture } from '../map/sprites';
import { FOG_ORDER, FogLayer } from './FogLayer';

const RELIEF = 7; // hauteur maximale du relief (unités de scène)
/** Ordre de rendu de ce qui passe au-dessus de la brume (la brume est à `FOG_ORDER`). */
const ABOVE_FOG = FOG_ORDER + 1;
/** Hauteur de la brume (unités de scène), juste au-dessus de la mer. */
const FOG_HEIGHT = 0.5;

interface Props {
  map: MapView;
  currentPlaceId: string | null;
  reachable: Set<string>;
  motion: PlayerMotion | null;
  playerPicture: string | null;
  playerName: string;
  onSelectPlace: (id: string) => void;
}

/** Relief de la carte : heightmap Azgaar si présente, sinon terrain généré ; aplani sous la brume. */
function useHeightmap(map: MapView): Heightmap {
  return useMemo(() => {
    let hm: Heightmap;
    if (map.terrain && map.terrain.cols > 1 && map.terrain.rows > 1) hm = map.terrain;
    else {
      const cols = 180;
      const rows = Math.max(2, Math.round((cols * map.height) / Math.max(1, map.width)));
      hm = proceduralTerrain(map, cols, rows);
    }
    const shapes = fogShapes(map);
    if (!shapes) return hm;
    return { ...hm, heights: flattenUnderFog(hm.heights, hm.cols, hm.rows, map.width, map.height, shapes, Math.round(LAND * 255)) };
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


/** Champ de vision vertical de la caméra (degrés). */
const FOV = 45;
/** Hauteur des noms de lieux et de l'épingle du joueur à l'écran (pixels CSS), sur tout écran. */
const LABEL_PX = 22;
const PIN_PX = 64;

/**
 * Contrôles de carte (glisser = déplacer, molette/pincer = zoom). Rendu à la demande.
 * Vue de départ : `frame` (les lieux, ou la zone découverte sous brouillard) cadré selon le format
 * de l'écran ; recadré si l'écran change (rotation, dépliage) tant que le joueur n'a pas bougé la vue.
 */
function Controls({ frame, floor, close }: { frame: SceneFrame; floor: number; close: boolean }) {
  const { camera, gl, invalidate, size, scene } = useThree();
  const aspect = size.width / Math.max(1, size.height);
  const moved = useRef(false);
  const controls = useRef<MapControls | null>(null);
  useEffect(() => {
    const c = new MapControls(camera, gl.domElement);
    c.enableDamping = false;
    c.maxPolarAngle = Math.PI / 2.5;
    c.minDistance = close ? 3 : 8;
    const onChange = () => invalidate();
    const onStart = () => {
      moved.current = true;
    };
    c.addEventListener('change', onChange);
    c.addEventListener('start', onStart);
    controls.current = c;
    moved.current = false;
    return () => {
      c.removeEventListener('change', onChange);
      c.removeEventListener('start', onStart);
      c.dispose();
      controls.current = null;
    };
  }, [camera, gl, invalidate, close, frame]);
  useEffect(() => {
    const c = controls.current;
    if (!c || moved.current) return;
    const elevation = (viewElevation(aspect) * Math.PI) / 180;
    const elevationSin = Math.sin(elevation);
    const dir = new THREE.Vector3(0, elevationSin, Math.cos(elevation));
    // Assez loin pour tout voir, et toujours au-dessus de la brume (`floor`).
    const d = Math.max(fitDistance(frame, FOV, aspect, elevationSin), (floor + 4) / elevationSin);
    c.maxDistance = Math.max(SCENE_SIZE * 1.6, d * 1.5);
    c.target.set(frame.x, 0, frame.z);
    camera.position.copy(dir).multiplyScalar(d).add(c.target);
    // Le brouillard de profondeur suit la distance de la caméra (sinon, de loin, tout est voilé).
    if (scene.fog instanceof THREE.Fog) {
      scene.fog.near = d + SCENE_SIZE * 0.4;
      scene.fog.far = d + SCENE_SIZE * 2;
    }
    c.update();
    invalidate();
  }, [camera, invalidate, frame, floor, aspect, scene]);
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

/** Routes : rubans qui suivent leur tracé (Azgaar) ou la ligne droite, posés sur le relief. */
function Roads({ map, hm, scale }: { map: MapView; hm: Heightmap; scale: number }) {
  const tubes = useMemo(
    () =>
      map.roads.flatMap((r) => {
        const pts = roadPath(map, r.from, r.to);
        return pts.length < 2 ? [] : [{ id: r.id, geometry: terrainTube(map, hm, pts, 0.22 * scale, 0.25), secret: r.secret }];
      }),
    [map, hm, scale],
  );
  useEffect(() => () => tubes.forEach((t) => t.geometry.dispose()), [tubes]);
  return (
    <>
      {tubes.map((t) => {
        // Sous brouillard, les routes envoyées (empruntées, ou qui partent d'ici vers une
        // destination) sont dessinées après la brume, qui n'écrit pas la profondeur : visibles
        // par-dessus, mais toujours cachées par le relief.
        const above = !!map.fog;
        return (
          <mesh key={t.id} geometry={t.geometry} renderOrder={above ? ABOVE_FOG : 0}>
            <meshLambertMaterial color={t.secret ? '#d4a72c' : '#efe6d2'} transparent={above} />
          </mesh>
        );
      })}
    </>
  );
}

/** Lieu : cercle à plat (pastille + contour) et son nom. */
function Place({ x, y, z, name, color, aboveFog, scale, onClick }: { x: number; y: number; z: number; name: string; color: string; aboveFog: boolean; scale: number; onClick: () => void }) {
  const label = useMemo(() => labelTexture(name), [name]);
  useEffect(() => () => label.texture.dispose(), [label]);
  // Taille fixe à l'écran (sinon, proportionnelle à la hauteur de l'écran : énorme sur un téléphone).
  const labelH = LABEL_PX / Math.max(1, useThree((st) => st.size.height));
  // Sous brouillard, la pastille et le nom sont dessinés après la brume : toujours lisibles.
  const order = aboveFog ? ABOVE_FOG + 1 : 0;
  return (
    <group position={[x, y, z]}>
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, 0.06, 0]} renderOrder={order} onClick={(e) => { e.stopPropagation(); onClick(); }}>
        <circleGeometry args={[1.25 * scale, 32]} />
        <meshBasicMaterial color={color} transparent={aboveFog} />
      </mesh>
      <mesh rotation={[-Math.PI / 2, 0, 0]} position={[0, 0.08, 0]} renderOrder={order}>
        <ringGeometry args={[1.25 * scale, 1.6 * scale, 32]} />
        <meshBasicMaterial color="#141518" transparent={aboveFog} />
      </mesh>
      <sprite position={[0, 0.8 + 1.6 * scale, 0]} scale={[labelH * label.aspect, labelH, 1]} renderOrder={aboveFog ? ABOVE_FOG + 2 : 0}>
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
  const h = PIN_PX / Math.max(1, useThree((st) => st.size.height));
  return (
    <sprite position={[x, y, z]} center={new THREE.Vector2(0.5, 0)} scale={[h * PIN_ASPECT, h, 1]} renderOrder={10}>
      <spriteMaterial map={texture} sizeAttenuation={false} depthTest={false} transparent />
    </sprite>
  );
}

/** Tube qui suit une polyligne de la carte, posé sur le relief (échantillonné à longueur égale). */
function terrainTube(map: MapView, hm: Heightmap, points: MapPoint[], radius: number, lift: number, steps = 48) {
  const pts: THREE.Vector3[] = [];
  for (let i = 0; i <= steps; i++) {
    const p = pointAlong(points, i / steps);
    const s = toScene(p.x, p.y, map.width, map.height);
    pts.push(new THREE.Vector3(s.x, elevation(hm, map, p.x, p.y) + lift, s.z));
  }
  // `getPointAt` (longueur d'arc) : la part affichée du tube suit la part parcourue.
  return new THREE.TubeGeometry(new THREE.CatmullRomCurve3(pts), steps * 2, radius, 8, false);
}

const RADIAL = 8;

/** Joueur : épingle animée en continu et partie parcourue du trajet. */
function Player({ map, hm, motion, picture, name }: { map: MapView; hm: Heightmap; motion: PlayerMotion; picture: string | null; name: string }) {
  const animated = useAnimatedPosition(motion);
  const traveled = useMemo(
    () => (motion.kind === 'road' ? terrainTube(map, hm, motion.path, 0.34, 0.3) : null),
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
  const scale = useMemo(() => markerScale(map), [map]);
  // Brume au ras de la mer : le relief est aplani sous elle (`useHeightmap`), les collines
  // découvertes en émergent et le trou de la brume tombe pile sur les lieux (pas de parallaxe).
  const fogHeight = FOG_HEIGHT;
  // Vue de départ : la zone découverte sous brouillard, sinon les lieux.
  const fogFrame = useMemo<SceneFrame | null>(() => {
    const r = fogFocus(map);
    if (!r) return null;
    const side = Math.max(map.width, map.height) || 1;
    const c = toScene(r.x + r.w / 2, r.y + r.h / 2, map.width, map.height);
    return { x: c.x, z: c.z, w: (r.w / side) * SCENE_SIZE, d: (r.h / side) * SCENE_SIZE };
    // Recadrer seulement quand la zone découverte change, pas à chaque rafraîchissement.
  }, [map.width, map.height, JSON.stringify(map.fog ?? null)]);
  const placesKey = map.places.map((p) => p.id).join(',');
  const frame = useMemo(() => fogFrame ?? placesFrame(map), [fogFrame, map.width, map.height, placesKey]);
  return (
    <Canvas
      frameloop="demand"
      dpr={[1, 1.5]}
      camera={{ position: [0, SCENE_SIZE * 0.62, SCENE_SIZE * 0.55], fov: FOV, near: 0.5, far: SCENE_SIZE * 8 }}
      gl={{ antialias: true, powerPreference: 'low-power' }}
    >
      <color attach="background" args={['#16324f']} />
      <fog attach="fog" args={['#16324f', SCENE_SIZE * 1.1, SCENE_SIZE * 2.6]} />
      <hemisphereLight args={['#fff8ec', '#3a4250', 1.25]} />
      <directionalLight position={[-40, 55, 25]} intensity={1.1} />
      <Controls frame={frame} floor={fogHeight} close={!!fogFrame} />
      <Terrain map={map} hm={hm} />
      <Roads map={map} hm={hm} scale={scale} />
      {map.places.map((p) => {
        const s = toScene(p.x, p.y, map.width, map.height);
        const color = p.id === currentPlaceId ? '#57f287' : reachable.has(p.id) ? '#5865f2' : '#f2f3f5';
        return <Place key={p.id} x={s.x} y={elevation(hm, map, p.x, p.y)} z={s.z} name={p.name} color={color} aboveFog={!!map.fog} scale={scale} onClick={() => onSelectPlace(p.id)} />;
      })}
      {motion && <Player map={map} hm={hm} motion={motion} picture={playerPicture} name={playerName} />}
      {/* Brouillard (E8) : au-dessus du plus haut relief. */}
      {map.fog && <FogLayer map={map} height={fogHeight} />}
    </Canvas>
  );
}
