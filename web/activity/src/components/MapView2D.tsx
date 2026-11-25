import { useEffect, useId, useMemo, useRef, useState } from 'react';
import { apiBlobUrl, type MapView } from '../api';
import { frameView, placesBox, roadPath, splitAlong, type MapPoint, type PlayerMotion } from '../map/geometry';
import { useAnimatedPosition } from '../map/useMotion';
import { initials, pinPath } from '../map/player';
import { Fog2D } from './Fog2D';
import { fitRect, fogFocus } from '../map/fog';
import { anchorPath, burgFont, burgRadius, declutterLabels, reliefRaster, riverOutline, seaCss, styleRaster, upscaleFactor, type Raster } from '../map/azgaarStyle';
import { vectorLayers } from '../map/azgaarVector';
import { proceduralTerrain, type Heightmap } from '../map/procedural';
import type { Translator } from '../i18n';

import type { OtherGroup } from '../map/motion';

interface Props {
  map: MapView;
  currentPlaceId: string | null;
  reachable: Set<string>;
  motion: PlayerMotion | null;
  playerPicture: string | null;
  playerName: string;
  onSelectPlace: (id: string) => void;
  /** Autres groupes (mode spectateur). */
  others?: OtherGroup[];
  /** Libellés des réglages du rendu (États, parchemin) ; sans lui, pas de réglages. */
  t?: Translator;
}

interface Box { x: number; y: number; w: number; h: number }

const toAttr = (pts: MapPoint[]) => pts.map((p) => `${p.x},${p.y}`).join(' ');

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

/** Réglages du rendu, propres à chaque joueur (stockage local, facultatif). */
function usePreference(key: string, initial: boolean): [boolean, (v: boolean) => void] {
  const [value, setValue] = useState(() => {
    try {
      const v = localStorage.getItem(key);
      return v === null ? initial : v === '1';
    } catch {
      return initial;
    }
  });
  const set = (v: boolean) => {
    setValue(v);
    try {
      localStorage.setItem(key, v ? '1' : '0');
    } catch {
      /* stockage indisponible : le réglage vaut pour la séance */
    }
  };
  return [value, set];
}

/** Image `data:` d'un rendu (une case = un pixel, agrandie et lissée par le navigateur). */
function rasterUrl(raster: Raster | null): string | null {
  if (!raster || typeof document === 'undefined') return null;
  try {
    const canvas = document.createElement('canvas');
    canvas.width = raster.width;
    canvas.height = raster.height;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    ctx.putImageData(new ImageData(raster.data, raster.width, raster.height), 0, 0);
    // WebP : bien plus léger et rapide à produire qu'un PNG pour une image de cette taille
    // (le navigateur revient au PNG s'il ne sait pas l'écrire).
    return canvas.toDataURL('image/webp', 0.92);
  } catch {
    return null;
  }
}

/** Relief de repli (carte sans rendu Azgaar) : heightmap Azgaar, sinon terrain généré. */
function fallbackHeightmap(map: MapView): Heightmap {
  if (map.terrain && map.terrain.cols > 1 && map.terrain.rows > 1) return map.terrain;
  const cols = 240;
  return proceduralTerrain(map, cols, Math.max(2, Math.round((cols * map.height) / Math.max(1, map.width))));
}

/**
 * Carte 2D dans le repère de la carte. Avec l'image exportée par Azgaar (hors brouillard), elle
 * est affichée telle quelle. Sinon, rendu façon Azgaar d'après les données du serveur (biomes,
 * mers, rivières, États, icônes des bourgs), ou d'après le relief pour une carte sans données
 * Azgaar. Glisser = déplacer, molette ou pincer = zoom.
 */
/** Épingle d'un autre groupe (spectateur) : initiales et nom, animée comme celle du joueur. */
function OtherPin({ group, unit, pinR, pinH }: { group: OtherGroup; unit: number; pinR: number; pinH: number }) {
  const at = useAnimatedPosition(group.motion)?.at;
  if (!at) return null;
  return (
    <g transform={`translate(${at.x} ${at.y})`} className="map-pin other" pointerEvents="none">
      <path d={pinPath(pinR, pinH)} fill="#faa61a" stroke="#141518" strokeWidth={1.5 * unit} />
      <circle cx={0} cy={-pinH + pinR} r={pinR - 3 * unit} fill="#4e5058" />
      <text x={0} y={-pinH + pinR} fontSize={11 * unit} textAnchor="middle" dominantBaseline="central" fill="#fff" fontWeight={700}>
        {initials(group.name)}
      </text>
      <text x={0} y={-pinH - 5 * unit} fontSize={12 * unit} textAnchor="middle" className="map-label" strokeWidth={3 * unit} paintOrder="stroke">
        {group.name}
      </text>
    </g>
  );
}

export function MapView2D({ map, currentPlaceId, reachable, motion, playerPicture, playerName, onSelectPlace, others, t }: Props) {
  const animated = useAnimatedPosition(motion);
  const player = animated?.at ?? null;
  const background = useBackground(map.background?.url ?? null);
  const hasStates = !!map.style?.states && (map.style.state_colors?.some((c) => !!c) ?? false);
  const [showStates, setShowStates] = usePreference('ve.map.states', true);
  const [paper, setPaper] = usePreference('ve.map.parchment', false);
  const full: Box = { x: 0, y: 0, w: map.width, h: map.height };
  const [box, setBox] = useState<Box>(full);
  const [px, setPx] = useState(800); // largeur affichée, pour garder une taille de texte constante
  const ref = useRef<SVGSVGElement>(null);
  const pointers = useRef(new Map<number, { x: number; y: number }>());
  const [imageFailed, setImageFailed] = useState(false);

  // Cadrage de départ (une fois par carte, quand la taille de la vue est connue) : les lieux et
  // une marge remplissent la vue (aucun nom ne touche le bord). Ensuite, c'est le joueur qui
  // déplace et zoome. Sous brouillard, la vue cadre la zone découverte (sur une grande carte, ce ne
  // serait qu'un point).
  const framed = useRef<string>('');
  const focus = fogFocus(map);
  const focusKey = focus ? `${focus.x},${focus.y},${focus.w},${focus.h}` : '';
  useEffect(() => {
    const el = ref.current;
    const key = `${map.width}x${map.height}|${focusKey}`;
    if (!el || framed.current === key || !el.clientWidth || !el.clientHeight) return;
    // Le joueur a une position : on attend qu'elle soit calculée pour centrer dessus.
    if (motion && !player && !focus) return;
    framed.current = key;
    setBox(focus ? fitRect(focus, el.clientWidth, el.clientHeight) : frameView(placesBox(map), el.clientWidth, el.clientHeight, player));
    // `focus` est recalculé à chaque rendu : `focusKey` suffit ; les lieux ne recadrent pas la vue
    // à chaque rafraîchissement de la carte.
  }, [map.width, map.height, px, player, motion, focusKey]);
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

  const pinR = 17 * unit;
  const pinH = 50 * unit;
  const showImage = background && !imageFailed;
  // Rendu façon Azgaar (ou relief de repli), seulement sans l'image d'Azgaar.
  // Recalculé seulement si ce qui est dessiné change (pas à chaque rafraîchissement de la carte).
  const political = hasStates && showStates;
  const rasterKey = map.background
    ? ''
    : map.style
      ? `${map.style.surface}|${map.style.states ?? ''}|${map.terrain?.heights.length ?? 0}|${political}|${paper}`
      : `${JSON.stringify(map.terrain?.heights ?? map.places.map((p) => [p.id, p.x, p.y]))}|${map.width}x${map.height}|${paper}`;
  const terrainUrl = useMemo(() => {
    if (map.background) return null;
    if (map.style) return rasterUrl(styleRaster(map.style, map.terrain, { states: political, parchment: paper, landAsSea: !!map.style.vector }, upscaleFactor(map.style.cols)));
    const hm = fallbackHeightmap(map);
    return rasterUrl(reliefRaster(hm, { parchment: paper, soft: hm !== map.terrain }, upscaleFactor(hm.cols)));
    // `rasterKey` résume tout ce qui est lu ici.
  }, [rasterKey]);
  // Noms sans chevauchement à l'échelle courante (recalculés par paliers de zoom).
  const zoomStep = Math.round(Math.log2(unit) * 6);
  const labelled = useMemo(
    () =>
      declutterLabels(
        map.places.map((p) => ({
          id: p.id, x: p.x, y: p.y, name: p.name,
          radius: burgRadius(p.burg) * unit, font: burgFont(p.burg) * unit,
          priority: (p.id === currentPlaceId ? 1000 : reachable.has(p.id) ? 500 : 0) + (p.burg?.capital ? 100 : 0) + (p.burg?.size ?? 1) * 10,
        })),
      ),
    // `unit` varie avec `zoomStep`.
    [map.places, zoomStep, currentPlaceId, reachable],
  );
  const rivers = useMemo(() => (map.style?.rivers ?? []).map((rv) => riverOutline(rv.points, rv.width)).filter(Boolean), [map.style]);
  const sea = seaCss(paper);
  const vectorId = useId();
  const vector = useMemo(() => (map.style?.vector && !map.background ? vectorLayers(map.style, { states: political, parchment: paper }) : null), [map.style, political, paper]);
  // Sous brouillard, les routes envoyées (empruntées, ou vers une destination) sont dessinées
  // par-dessus la brume : le serveur n'envoie que celles-là.
  const aboveFog = (road: MapView['roads'][number]) => !!map.fog && !!road;
  const drawRoad = (road: MapView['roads'][number]) => {
    // Avec Azgaar, les routes sont déjà dessinées sur la carte : seule la route secrète connue est marquée.
    if (showImage && !road.secret) return null;
    const pts = roadPath(map, road.from, road.to);
    if (pts.length < 2) return null;
    // Sinon, routes brunes d'Azgaar sur un liseré clair (lisibles sur la brume comme sur la mer).
    return (
      <g key={road.id}>
        {!road.secret && (
          <polyline points={toAttr(pts)} fill="none" stroke="#f3ead2" strokeOpacity={0.75} strokeWidth={4 * unit} strokeLinecap="round" strokeLinejoin="round" />
        )}
        <polyline points={toAttr(pts)} fill="none"
          stroke={road.secret ? '#d4a72c' : '#8d502a'} strokeWidth={(road.secret ? 3 : 1.8) * unit} strokeDasharray={road.secret ? `${6 * unit} ${4 * unit}` : undefined}
          strokeLinecap="round" strokeLinejoin="round" />
      </g>
    );
  };
  return (
    <>
    <svg
      ref={ref}
      className={`map-2d${showImage ? ' with-background' : ''}${paper ? ' parchment' : ''}`}
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
      {!map.background && (
        <g className="map-terrain" pointerEvents="none">
          {/* Le large autour de la carte : les marges du cadrage ne sont pas un vide. */}
          <rect x={-map.width} y={-map.height} width={map.width * 3} height={map.height * 3} fill={sea} />
          {terrainUrl && <image href={terrainUrl} x={0} y={0} width={map.width} height={map.height} preserveAspectRatio="none" />}
          {vector && (
            <g pointerEvents="none">
              {vector.land && <clipPath id={`${vectorId}-land`}><path d={vector.land} /></clipPath>}
              <g clipPath={vector.land ? `url(#${vectorId}-land)` : undefined}>
                <rect x={0} y={0} width={map.width} height={map.height} fill={vector.landFill} />
                {vector.biomes.map((b, i) => <path key={i} d={b.d} fill={b.fill} stroke={b.fill} strokeWidth={0.6 * unit} strokeLinejoin="round" />)}
                {vector.states.map((s, i) => <path key={i} d={s.d} fill={s.fill} fillOpacity={s.opacity} />)}
              </g>
              {vector.lakes && <path d={vector.lakes.d} fill={vector.lakes.fill} stroke={vector.lakes.stroke} strokeWidth={0.8 * unit} strokeLinejoin="round" />}
              {vector.coast && <path d={vector.coast.d} fill="none" stroke={vector.coast.stroke} strokeWidth={1.2 * unit} strokeLinejoin="round" strokeLinecap="round" />}
              {vector.borders && (
                <path d={vector.borders.d} fill="none" stroke={vector.borders.stroke} strokeWidth={1 * unit}
                  strokeDasharray={`${4 * unit} ${2 * unit}`} strokeLinejoin="round" strokeLinecap="round" />
              )}
            </g>
          )}
          {rivers.length > 0 && (
            <g fill={paper ? '#7d8f99' : '#5d97bb'} stroke={paper ? '#7d8f99' : '#5d97bb'} strokeWidth={0.8 * unit} strokeLinejoin="round">
              {rivers.map((d, i) => <path key={i} d={d} />)}
            </g>
          )}
        </g>
      )}
      {map.roads.filter((road) => !aboveFog(road)).map(drawRoad)}
      {motion?.kind === 'road' && animated?.progress != null && (() => {
        // Trajet le long du tracé de la route : parcouru en plein, restant en pointillés.
        const { traveled, remaining } = splitAlong(motion.path, animated.progress);
        return (
          <g className="map-progress">
            <polyline points={toAttr(remaining)} fill="none" stroke="#f2f3f5" strokeWidth={3 * unit} strokeDasharray={`${2 * unit} ${6 * unit}`} strokeLinecap="round" strokeLinejoin="round" />
            <polyline points={toAttr(traveled)} fill="none" stroke="#141518" strokeWidth={7 * unit} strokeLinecap="round" strokeLinejoin="round" />
            <polyline points={toAttr(traveled)} fill="none" stroke="#5865f2" strokeWidth={4.5 * unit} strokeLinecap="round" strokeLinejoin="round" />
          </g>
        );
      })()}
      {/* Brouillard (E8) : sous les routes et les lieux envoyés, qui restent lisibles. */}
      <Fog2D map={map} />
      {map.roads.filter(aboveFog).map(drawRoad)}
      {map.places.map((p) => {
        const fill = p.id === currentPlaceId ? '#57f287' : reachable.has(p.id) ? '#5865f2' : '#f2f3f5';
        // Icône du bourg façon Azgaar : cercle selon la population, carré pour une capitale, ancre des ports.
        const r = burgRadius(p.burg) * unit;
        const font = burgFont(p.burg) * unit;
        return (
          <g key={p.id} className="map-place" onClick={() => onSelectPlace(p.id)}>
            {p.burg?.capital ? (
              <rect x={p.x - r * 0.85} y={p.y - r * 0.85} width={r * 1.7} height={r * 1.7} fill={fill} stroke="#3e3e4b" strokeWidth={2 * unit} />
            ) : (
              <circle cx={p.x} cy={p.y} r={r} fill={fill} stroke="#3e3e4b" strokeWidth={2 * unit} />
            )}
            {p.burg?.port && (
              <path d={anchorPath(4 * unit)} transform={`translate(${p.x + r + 5 * unit} ${p.y + r * 0.6})`} fill="none"
                stroke="#3e3e4b" strokeWidth={1.4 * unit} strokeLinecap="round" />
            )}
            {labelled.has(p.id) && (
              <text x={p.x} y={p.y - r - 5 * unit} fontSize={font} textAnchor="middle"
                className={`map-label azgaar${p.burg?.capital ? ' capital' : ''}`} strokeWidth={3.2 * unit} paintOrder="stroke">
                {p.name}
              </text>
            )}
          </g>
        );
      })}
      {others?.map((o) => <OtherPin key={o.key} group={o} unit={unit} pinR={pinR * 0.75} pinH={pinH * 0.75} />)}
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
    {t && !map.background && (
      <div className="map-style-controls">
        {hasStates && (
          <button type="button" className={showStates ? 'active' : ''} aria-pressed={showStates} onClick={() => setShowStates(!showStates)}>
            {t('activity_ui__map_states')}
          </button>
        )}
        <button type="button" className={paper ? 'active' : ''} aria-pressed={paper} onClick={() => setPaper(!paper)}>
          {t('activity_ui__map_parchment')}
        </button>
      </div>
    )}
    </>
  );
}
