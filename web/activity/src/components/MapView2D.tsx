import { useEffect, useRef, useState } from 'react';
import { apiBlobUrl, type MapView } from '../api';
import type { PlayerMotion } from '../map/geometry';
import { useAnimatedPosition } from '../map/useMotion';
import { initials, pinPath } from '../map/player';

interface Props {
  map: MapView;
  currentPlaceId: string | null;
  reachable: Set<string>;
  motion: PlayerMotion | null;
  playerPicture: string | null;
  playerName: string;
  onSelectPlace: (id: string) => void;
}

interface Box { x: number; y: number; w: number; h: number }

/** Image Azgaar chargée avec le jeton de session, puis affichée via une URL `blob:` locale. */
function useBackground(url: string | null): string | null {
  const [blob, setBlob] = useState<string | null>(null);
  useEffect(() => {
    if (!url) return setBlob(null);
    let revoked = false;
    let current: string | null = null;
    apiBlobUrl(url).then((u) => {
      if (revoked) return URL.revokeObjectURL(u);
      current = u;
      setBlob(u);
    }).catch(() => setBlob(null));
    return () => {
      revoked = true;
      if (current) URL.revokeObjectURL(current);
    };
  }, [url]);
  return blob;
}

/**
 * Carte 2D dans le repère de la carte. Avec un export Azgaar, l'image est affichée telle quelle
 * sous les lieux : mêmes visuels qu'Azgaar. Glisser = déplacer, molette ou pincer = zoom.
 */
export function MapView2D({ map, currentPlaceId, reachable, motion, playerPicture, playerName, onSelectPlace }: Props) {
  const animated = useAnimatedPosition(motion);
  const player = animated?.at ?? null;
  const background = useBackground(map.background?.url ?? null);
  const full: Box = { x: 0, y: 0, w: map.width, h: map.height };
  const [box, setBox] = useState<Box>(full);
  const [px, setPx] = useState(800); // largeur affichée, pour garder une taille de texte constante
  const ref = useRef<SVGSVGElement>(null);
  const pointers = useRef(new Map<number, { x: number; y: number }>());
  const [imageFailed, setImageFailed] = useState(false);

  useEffect(() => setBox({ x: 0, y: 0, w: map.width, h: map.height }), [map.width, map.height]);
  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setPx(el.clientWidth || 800));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  /** Taille à l'écran : la largeur visible représente `px` pixels. */
  const unit = Math.max(box.w / px, box.h / ((ref.current?.clientHeight || px * 0.6) || 1));

  const toMap = (clientX: number, clientY: number) => {
    const r = ref.current!.getBoundingClientRect();
    const scale = Math.max(box.w / r.width, box.h / r.height);
    const offX = (r.width * scale - box.w) / 2;
    const offY = (r.height * scale - box.h) / 2;
    return { x: box.x - offX + (clientX - r.left) * scale, y: box.y - offY + (clientY - r.top) * scale };
  };
  const zoomAt = (cx: number, cy: number, factor: number) => {
    setBox((b) => {
      const w = Math.min(map.width * 1.5, Math.max(map.width / 25, b.w * factor));
      const h = (w / b.w) * b.h;
      return { x: cx - ((cx - b.x) * w) / b.w, y: cy - ((cy - b.y) * h) / b.h, w, h };
    });
  };

  const onWheel = (e: React.WheelEvent) => {
    const p = toMap(e.clientX, e.clientY);
    zoomAt(p.x, p.y, e.deltaY > 0 ? 1.15 : 1 / 1.15);
  };
  const onPointerDown = (e: React.PointerEvent) => {
    (e.target as Element).setPointerCapture?.(e.pointerId);
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
  };
  const onPointerMove = (e: React.PointerEvent) => {
    const prev = pointers.current.get(e.pointerId);
    if (!prev) return;
    const all = [...pointers.current.entries()];
    if (all.length === 2) {
      // Pincement : rapport des écarts entre les deux doigts.
      const other = all.find(([id]) => id !== e.pointerId)![1];
      const before = Math.hypot(prev.x - other.x, prev.y - other.y);
      const after = Math.hypot(e.clientX - other.x, e.clientY - other.y);
      if (before > 0 && after > 0) {
        const mid = toMap((e.clientX + other.x) / 2, (e.clientY + other.y) / 2);
        zoomAt(mid.x, mid.y, before / after);
      }
    } else {
      const r = ref.current!.getBoundingClientRect();
      const scale = Math.max(box.w / r.width, box.h / r.height);
      setBox((b) => ({ ...b, x: b.x - (e.clientX - prev.x) * scale, y: b.y - (e.clientY - prev.y) * scale }));
    }
    pointers.current.set(e.pointerId, { x: e.clientX, y: e.clientY });
  };
  const onPointerUp = (e: React.PointerEvent) => pointers.current.delete(e.pointerId);

  const byId = new Map(map.places.map((p) => [p.id, p]));
  const r = 7 * unit;
  const font = 13 * unit;
  const pinR = 17 * unit;
  const pinH = 50 * unit;
  const showImage = background && !imageFailed;
  return (
    <svg
      ref={ref}
      className={`map-2d${showImage ? ' with-background' : ''}`}
      viewBox={`${box.x} ${box.y} ${box.w} ${box.h}`}
      onWheel={onWheel}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={onPointerUp}
      role="img"
    >
      <defs>
        <clipPath id="pin-photo">
          <circle cx={0} cy={-pinH + pinR} r={pinR - 3 * unit} />
        </clipPath>
      </defs>
      {showImage && (
        <image href={background} x={0} y={0} width={map.width} height={map.height} preserveAspectRatio="none" onError={() => setImageFailed(true)} />
      )}
      {map.roads.map((road) => {
        const a = byId.get(road.from);
        const b = byId.get(road.to);
        if (!a || !b) return null;
        // Avec Azgaar, les routes sont déjà dessinées sur l'image : seule la route secrète connue est marquée.
        if (showImage && !road.secret) return null;
        return (
          <line key={road.id} x1={a.x} y1={a.y} x2={b.x} y2={b.y}
            stroke={road.secret ? '#d4a72c' : '#d9d9d9'} strokeWidth={3 * unit} strokeDasharray={road.secret ? `${6 * unit} ${4 * unit}` : undefined} strokeLinecap="round" />
        );
      })}
      {motion?.kind === 'road' && player && (
        // Trajet : la partie parcourue en plein, le reste en pointillés.
        <g className="map-progress">
          <line x1={motion.to.x} y1={motion.to.y} x2={player.x} y2={player.y} stroke="#f2f3f5" strokeWidth={3 * unit} strokeDasharray={`${2 * unit} ${6 * unit}`} strokeLinecap="round" />
          <line x1={motion.from.x} y1={motion.from.y} x2={player.x} y2={player.y} stroke="#141518" strokeWidth={7 * unit} strokeLinecap="round" />
          <line x1={motion.from.x} y1={motion.from.y} x2={player.x} y2={player.y} stroke="#5865f2" strokeWidth={4.5 * unit} strokeLinecap="round" />
        </g>
      )}
      {map.places.map((p) => {
        const fill = p.id === currentPlaceId ? '#57f287' : reachable.has(p.id) ? '#5865f2' : '#f2f3f5';
        return (
          <g key={p.id} className="map-place" onClick={() => onSelectPlace(p.id)}>
            <circle cx={p.x} cy={p.y} r={r} fill={fill} stroke="#141518" strokeWidth={2.5 * unit} />
            <text x={p.x} y={p.y - r - 5 * unit} fontSize={font} textAnchor="middle" className="map-label"
              stroke="#141518" strokeWidth={3.5 * unit} paintOrder="stroke">
              {p.name}
            </text>
          </g>
        );
      })}
      {player && (
        <g transform={`translate(${player.x} ${player.y})`} className="map-pin">
          <path d={pinPath(pinR, pinH)} fill="#ed4245" stroke="#141518" strokeWidth={1.5 * unit} />
          {playerPicture ? (
            <image href={playerPicture} x={-pinR} y={-pinH} width={pinR * 2} height={pinR * 2} clipPath="url(#pin-photo)" preserveAspectRatio="xMidYMid slice" />
          ) : (
            <>
              <circle cx={0} cy={-pinH + pinR} r={pinR - 3 * unit} fill="#5865f2" />
              <text x={0} y={-pinH + pinR} fontSize={14 * unit} textAnchor="middle" dominantBaseline="central" fill="#fff" fontWeight={700}>
                {initials(playerName)}
              </text>
            </>
          )}
        </g>
      )}
    </svg>
  );
}
