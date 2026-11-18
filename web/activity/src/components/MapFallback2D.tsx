import type { MapView } from '../api';
import { toScene, SCENE_SIZE, type ScenePoint } from '../map/geometry';

interface Props {
  map: MapView;
  currentPlaceId: string | null;
  reachable: Set<string>;
  player: ScenePoint | null;
  onSelectPlace: (id: string) => void;
}

/** Carte 2D (SVG) quand WebGL est indisponible : mêmes données, mêmes interactions. */
export function MapFallback2D({ map, currentPlaceId, reachable, player, onSelectPlace }: Props) {
  const half = SCENE_SIZE / 2;
  const byId = new Map(map.places.map((p) => [p.id, toScene(p.x, p.y, map.width, map.height)]));
  return (
    <svg className="map-2d" viewBox={`${-half - 4} ${-half - 4} ${SCENE_SIZE + 8} ${SCENE_SIZE + 8}`} role="img">
      {map.roads.map((r) => {
        const a = byId.get(r.from);
        const b = byId.get(r.to);
        return a && b ? <line key={r.id} x1={a.x} y1={a.z} x2={b.x} y2={b.z} stroke={r.secret ? '#c9a227' : '#d9d9d9'} strokeWidth={0.4} /> : null;
      })}
      {map.places.map((p) => {
        const s = byId.get(p.id)!;
        const fill = p.id === currentPlaceId ? '#57f287' : reachable.has(p.id) ? '#5865f2' : '#b5bac1';
        return (
          <g key={p.id} onClick={() => onSelectPlace(p.id)} style={{ cursor: 'pointer' }}>
            <circle cx={s.x} cy={s.z} r={1.4} fill={fill} />
            <text x={s.x} y={s.z - 2} fontSize={2.4} textAnchor="middle" fill="#f2f3f5">{p.name}</text>
          </g>
        );
      })}
      {player && <circle cx={player.x} cy={player.z} r={1.2} fill="#fee75c" />}
    </svg>
  );
}
