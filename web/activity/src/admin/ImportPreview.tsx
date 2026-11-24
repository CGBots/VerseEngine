import { useEffect, useMemo, useRef, useState } from 'react';
import type { ImportPlan, UploadedMap } from '../api';
import type { Translator } from '../i18n';
import { roadKey, serverColor } from './importModel';

interface Props {
  map: UploadedMap;
  plan: ImportPlan | null;
  secret: Set<string>;
  onToggleSecret: (a: number, b: number) => void;
  t: Translator;
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
export function ImportPreview({ map, plan, secret, onToggleSecret, t }: Props) {
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
      className="import-preview"
      viewBox={`${box.x} ${box.y} ${box.w} ${box.h}`}
      preserveAspectRatio="xMidYMid meet"
      role="img"
      aria-label={t('activity_ui__import_preview')}
      onWheel={onWheel}
      onPointerDown={(e) => {
        drag.current = { x: e.clientX, y: e.clientY, box };
      }}
      onPointerMove={(e) => {
        const d = drag.current;
        if (!d || !e.buttons) return;
        const r = ref.current!.getBoundingClientRect();
        const s = d.box.w / r.width;
        setBox({ ...d.box, x: d.box.x - (e.clientX - d.x) * s, y: d.box.y - (e.clientY - d.y) * s });
      }}
      onPointerUp={() => (drag.current = null)}
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
      {places.map((p) => (
        <g key={p.burg_id}>
          <title>{p.name}</title>
          <circle cx={p.x} cy={p.y} r={(p.capital ? 6 : 4) * unit} fill={serverColor(p.server)} stroke="#111214" strokeWidth={unit} />
        </g>
      ))}
    </svg>
  );
}
