import { useEffect, useMemo, useRef, useState } from 'react';
import type { ImportPlan, UploadedMap } from '../api';
import type { Translator } from '../i18n';
import { roadKey, serverColor } from './importModel';

const toPoints = (pts: [number, number][]) => pts.map(([x, y]) => `${x},${y}`).join(' ');

interface Props {
  map: UploadedMap;
  plan: ImportPlan | null;
  secret: Set<string>;
  onToggleSecret: (a: number, b: number) => void;
  t: Translator;
  /** Mode lasso : glisser trace la zone à importer (au lieu de déplacer la vue). */
  lasso?: boolean;
  /** Zone tracée (polygone, coordonnées de la carte) ; vide : toute la carte. */
  zone?: [number, number][];
  /** Tracé terminé (points bruts, à simplifier). */
  onLasso?: (points: [number, number][]) => void;
}

interface Box { x: number; y: number; w: number; h: number }

/** Relief Azgaar (0–100, terre à partir de 20) en image : eau en bleu, terre du vert au brun. */
function useTerrainImage(terrain: UploadedMap['terrain']): string | null {
  return useMemo(() => {
    if (!terrain || terrain.cols < 2 || terrain.rows < 2 || typeof document === 'undefined') return null;
    const canvas = document.createElement('canvas');
    canvas.width = terrain.cols;
    canvas.height = terrain.rows;
    const ctx = canvas.getContext('2d');
    if (!ctx) return null;
    const img = ctx.createImageData(terrain.cols, terrain.rows);
    terrain.heights.forEach((h, i) => {
      const land = h >= 20;
      const k = land ? (h - 20) / 80 : h / 20;
      const [r, g, b] = land ? [60 + 120 * k, 110 + 40 * (1 - k), 60] : [20, 50 + 40 * k, 90 + 60 * k];
      img.data.set([r, g, b, 255], i * 4);
    });
    ctx.putImageData(img, 0, 0);
    return canvas.toDataURL();
  }, [terrain]);
}

/**
 * Aperçu de l'import : relief, lieux colorés par serveur, routes. Un clic sur une route la rend
 * secrète (ou publique) ; molette = zoom, glisser = déplacer.
 */
export function ImportPreview({ map, plan, secret, onToggleSecret, t, lasso = false, zone = [], onLasso }: Props) {
  // Tracé du lasso en cours (coordonnées de la carte).
  const [drawing, setDrawing] = useState<[number, number][] | null>(null);
  const toMap = (clientX: number, clientY: number): [number, number] | null => {
    const svg = ref.current;
    const m = svg?.getScreenCTM();
    if (!svg || !m) return null;
    const pt = new DOMPoint(clientX, clientY).matrixTransform(m.inverse());
    return [pt.x, pt.y];
  };
  const terrain = useTerrainImage(map.terrain);
  const full: Box = { x: 0, y: 0, w: map.width, h: map.height };
  const [box, setBox] = useState<Box>(full);
  const ref = useRef<SVGSVGElement>(null);
  const drag = useRef<{ x: number; y: number; box: Box } | null>(null);
  useEffect(() => setBox({ x: 0, y: 0, w: map.width, h: map.height }), [map.width, map.height]);

  const unit = box.w / (ref.current?.clientWidth || 800);
  const serverOf = useMemo(() => new Map(plan?.places.map((p) => [p.burg_id, p.server]) ?? []), [plan]);
  const places = plan?.places ?? map.burgs.map((b) => ({ burg_id: b.id, name: b.name, x: b.x, y: b.y, capital: b.capital, server: 0 }));

  const onWheel = (e: React.WheelEvent) => {
    const r = ref.current!.getBoundingClientRect();
    const fx = (e.clientX - r.left) / r.width;
    const fy = (e.clientY - r.top) / r.height;
    const k = e.deltaY > 0 ? 1.2 : 1 / 1.2;
    const w = Math.min(full.w * 2, Math.max(full.w / 40, box.w * k));
    const h = (w / box.w) * box.h;
    setBox({ x: box.x + (box.w - w) * fx, y: box.y + (box.h - h) * fy, w, h });
  };

  return (
    <svg
      ref={ref}
      viewBox={`${box.x} ${box.y} ${box.w} ${box.h}`}
      preserveAspectRatio="xMidYMid meet"
      role="img"
      aria-label={t('activity_ui__import_preview')}
      onWheel={onWheel}
      className={`import-preview${lasso ? ' lasso' : ''}`}
      onPointerDown={(e) => {
        if (lasso) {
          const p = toMap(e.clientX, e.clientY);
          if (!p) return;
          e.preventDefault();
          e.currentTarget.setPointerCapture(e.pointerId);
          setDrawing([p]);
          return;
        }
        drag.current = { x: e.clientX, y: e.clientY, box };
      }}
      onPointerMove={(e) => {
        if (lasso) {
          const p = drawing && toMap(e.clientX, e.clientY);
          if (p) setDrawing([...drawing!, p]);
          return;
        }
        const d = drag.current;
        if (!d || !e.buttons) return;
        const r = ref.current!.getBoundingClientRect();
        const s = d.box.w / r.width;
        setBox({ ...d.box, x: d.box.x - (e.clientX - d.x) * s, y: d.box.y - (e.clientY - d.y) * s });
      }}
      onPointerUp={() => {
        drag.current = null;
        if (lasso && drawing) {
          onLasso?.(drawing);
          setDrawing(null);
        }
      }}
    >
      <rect x={0} y={0} width={map.width} height={map.height} fill={map.generated ? '#2f4a33' : '#14324a'} />
      {terrain && <image href={terrain} x={0} y={0} width={map.width} height={map.height} preserveAspectRatio="none" />}
      {plan?.roads.map((r) => {
        const k = roadKey(r.burg_a, r.burg_b);
        const isSecret = secret.has(k);
        const cut = serverOf.get(r.burg_a) !== serverOf.get(r.burg_b);
        const points = r.path.map(([x, y]) => `${x},${y}`).join(' ');
        return (
          <g key={k} className="preview-road" onClick={() => onToggleSecret(r.burg_a, r.burg_b)}>
            <title>{`${r.distance} km${isSecret ? ` · ${t('activity_ui__import_secret')}` : ''}${cut ? ` · ${t('activity_ui__import_cross_server')}` : ''}`}</title>
            {/* Zone de clic plus large que le trait. */}
            <polyline points={points} fill="none" stroke="transparent" strokeWidth={10 * unit} />
            <polyline
              points={points}
              fill="none"
              stroke={isSecret ? '#f23f43' : cut ? '#f0b232' : '#f2f3f5'}
              strokeWidth={(cut ? 3 : 2) * unit}
              strokeDasharray={isSecret || r.uses_sea ? `${6 * unit} ${4 * unit}` : undefined}
              opacity={0.9}
            />
          </g>
        );
      })}
      {/* Zone du lasso : le reste de la carte est assombri (il ne sera pas importé). */}
      {zone.length >= 3 && (
        <g className="import-zone" pointerEvents="none">
          <path
            d={`M${-map.width} ${-map.height}H${map.width * 2}V${map.height * 2}H${-map.width}Z M${zone.map(([x, y]) => `${x} ${y}`).join(' L')}Z`}
            fill="rgb(0 0 0 / .55)"
            fillRule="evenodd"
          />
          <polygon points={toPoints(zone)} fill="none" stroke="#f0b232" strokeWidth={2 * unit} strokeDasharray={`${6 * unit} ${4 * unit}`} />
        </g>
      )}
      {drawing && drawing.length > 1 && (
        <polyline points={toPoints(drawing)} fill="rgb(240 178 50 / .15)" stroke="#f0b232" strokeWidth={2 * unit} pointerEvents="none" />
      )}
      {places.map((p) => (
        <g key={p.burg_id}>
          <title>{p.name}</title>
          <circle cx={p.x} cy={p.y} r={(p.capital ? 6 : 4) * unit} fill={serverColor(p.server)} stroke="#111214" strokeWidth={unit} />
        </g>
      ))}
    </svg>
  );
}
