import { useEffect, useMemo, useRef } from 'react';
import type { MapView } from '../api';
import { fogShapes } from '../map/fog';

/**
 * Brume de la carte 2D (Q4) : opaque et animée (bruit fractal qui dérive), dégagée aux lieux
 * découverts, aux destinations entrevues et le long des routes empruntées. Rien sans brouillard.
 */
export function Fog2D({ map }: { map: MapView }) {
  const shapes = useMemo(() => fogShapes(map), [map]);
  const drift = useRef<SVGGElement>(null);
  const turbulence = useRef<SVGFETurbulenceElement>(null);
  const active = !!shapes && !!map.fog;
  // Animation : la brume dérive et ses volutes respirent (15 images par seconde, sans rendu React).
  useEffect(() => {
    if (!active) return;
    const base = Number(turbulence.current?.getAttribute('baseFrequency')) || 0.02;
    const start = performance.now();
    const id = window.setInterval(() => {
      const t = (performance.now() - start) / 1000;
      drift.current?.setAttribute('transform', `translate(${(Math.sin(t / 19) * 0.06 * map.width).toFixed(2)} ${(Math.cos(t / 23) * 0.04 * map.height).toFixed(2)})`);
      turbulence.current?.setAttribute('baseFrequency', String(base * (1 + 0.12 * Math.sin(t / 7))));
    }, 1000 / 15);
    return () => window.clearInterval(id);
  }, [active, map.width, map.height]);
  if (!shapes || !map.fog) return null;
  const { width: w, height: h } = map;
  const big = { x: -w, y: -h, width: 3 * w, height: 3 * h };
  // Fréquence du bruit : quelques volutes par zone découverte.
  const freq = Math.min(0.2, 1.2 / Math.max(1, map.fog.radius));
  return (
    <g className="fog-2d" pointerEvents="none">
      <defs>
        <radialGradient id="fog-hole">
          <stop offset="0" stopColor="#000" />
          <stop offset="0.55" stopColor="#000" stopOpacity="0.9" />
          <stop offset="1" stopColor="#000" stopOpacity="0" />
        </radialGradient>
        <filter id="fog-soft" filterUnits="userSpaceOnUse" {...big}>
          <feGaussianBlur stdDeviation={map.fog.radius * 0.18} />
        </filter>
        <filter id="fog-noise" filterUnits="userSpaceOnUse" {...big}>
          <feTurbulence ref={turbulence} type="fractalNoise" baseFrequency={freq} numOctaves={4} seed={7} />
          {/* Gris-bleu clair, opacité variable par-dessus la base opaque. */}
          <feColorMatrix type="matrix" values="0 0 0 0 0.88  0 0 0 0 0.9  0 0 0 0 0.94  0.9 0 0 0 -0.15" />
        </filter>
        <mask id="fog-mask" maskUnits="userSpaceOnUse" {...big}>
          <rect {...big} fill="#fff" />
          {shapes.corridors.map((c, i) => (
            <polyline key={i} points={c.points.map((p) => `${p.x},${p.y}`).join(' ')} fill="none" stroke="#000"
              strokeWidth={c.width} strokeLinecap="round" strokeLinejoin="round" filter="url(#fog-soft)" />
          ))}
          {shapes.circles.map((c, i) => (
            <circle key={i} cx={c.x} cy={c.y} r={c.r} fill="url(#fog-hole)" />
          ))}
        </mask>
      </defs>
      <g mask="url(#fog-mask)">
        <rect {...big} fill="#8f98a4" />
        <g ref={drift}>
          <rect {...big} filter="url(#fog-noise)" />
        </g>
      </g>
    </g>
  );
}
