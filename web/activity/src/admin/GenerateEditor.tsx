import { lazy, Suspense, useEffect, useMemo, useRef, useState } from 'react';
import { ApiError, importApi, type GeneratedMap, type UploadedMap } from '../api';
import type { Translator } from '../i18n';
import { webglAvailable } from '../map/geometry';
import {
  addPlace, addRoad, deletePlace, deleteRoad, mapProblem, MAX_PLACES, MIN_PLACES, randomSeed, renamePlace, reshapeRoad, roadMiddle, roadPoints, toMapView,
} from './generateModel';

const MapScene = lazy(() => import('../components/MapScene').then((m) => ({ default: m.MapScene })));

type Tool = 'select' | 'add_place' | 'add_road' | 'delete' | 'reshape';
const TOOL_LABEL: Record<Tool, string> = {
  select: 'activity_ui__generate_tool_select',
  add_place: 'activity_ui__generate_tool_add_place',
  add_road: 'activity_ui__generate_tool_add_road',
  delete: 'activity_ui__generate_tool_delete',
  reshape: 'activity_ui__generate_tool_reshape',
};
const errKey = (e: unknown) => (e instanceof ApiError ? e.key : 'activity__internal_error');

interface Props {
  t: Translator;
  onReady: (map: UploadedMap) => void;
  onBack: () => void;
}

/**
 * « Générer une map » (I11) : nombre de lieux, graine affichée (et nouvelle graine), puis
 * retouches simples : renommer, ajouter ou supprimer des lieux et des routes, retoucher les
 * tracés. Vue 2D pour éditer, vue 3D détaillée de l'activité pour voir le rendu.
 */
export function GenerateEditor({ t, onReady, onBack }: Props) {
  const [count, setCount] = useState(30);
  const [seed, setSeed] = useState<number>(() => randomSeed());
  const [map, setMap] = useState<GeneratedMap | null>(null);
  const [tool, setTool] = useState<Tool>('select');
  const [selected, setSelected] = useState<number | null>(null);
  const [roadStart, setRoadStart] = useState<number | null>(null);
  const [view3d, setView3d] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const svg = useRef<SVGSVGElement>(null);
  const dragging = useRef<{ a: number; b: number } | null>(null);
  const can3d = useRef(webglAvailable()).current;

  const generate = async (s: number) => {
    setBusy(true);
    setError(null);
    try {
      const m = await importApi.generate(s, count);
      setMap(m);
      setSeed(m.seed);
      setSelected(null);
      setRoadStart(null);
    } catch (e) {
      setError(errKey(e));
    } finally {
      setBusy(false);
    }
  };
  useEffect(() => {
    void generate(seed);
    // Première carte à l'ouverture seulement.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const toMap = (e: { clientX: number; clientY: number }): [number, number] => {
    const el = svg.current!;
    const pt = el.createSVGPoint();
    pt.x = e.clientX;
    pt.y = e.clientY;
    const p = pt.matrixTransform(el.getScreenCTM()!.inverse());
    return [p.x, p.y];
  };

  const onPlace = (id: number) => {
    if (!map) return;
    if (tool === 'delete') setMap(deletePlace(map, id));
    else if (tool === 'add_road') {
      if (roadStart === null) setRoadStart(id);
      else {
        setMap(addRoad(map, roadStart, id));
        setRoadStart(null);
      }
    } else setSelected(id);
  };

  const sceneMap = useMemo(() => (map && view3d ? toMapView(map) : null), [map, view3d]);
  const problem = map ? mapProblem(map) : 'activity_ui__loading';
  const place = map?.places.find((p) => p.id === selected) ?? null;

  const useMap = async () => {
    if (!map) return;
    setBusy(true);
    setError(null);
    try {
      onReady(await importApi.useGenerated(map));
    } catch (e) {
      setError(errKey(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="import-options">
      <div className="generate-canvas">
        {map && view3d && sceneMap ? (
          <Suspense fallback={<p className="center">{t('activity_ui__loading')}</p>}>
            <MapScene map={sceneMap} currentPlaceId={null} reachable={new Set()} motion={null} playerPicture={null} playerName="" onSelectPlace={() => {}} />
          </Suspense>
        ) : (
          map && (
            <svg
              ref={svg}
              className={`import-preview generate-${tool}`}
              viewBox={`0 0 ${map.width} ${map.height}`}
              onClick={(e) => {
                if (tool === 'add_place' && e.target === e.currentTarget) {
                  const [x, y] = toMap(e);
                  setMap(addPlace(map, x, y, t('activity_ui__generate_new_place', { n: map.places.length + 1 })));
                }
              }}
              onPointerMove={(e) => {
                const d = dragging.current;
                if (d && e.buttons) setMap((m) => (m ? reshapeRoad(m, d.a, d.b, toMap(e)) : m));
              }}
              onPointerUp={() => (dragging.current = null)}
            >
              <rect x={0} y={0} width={map.width} height={map.height} fill="#2f4a33" pointerEvents="none" />
              {map.roads.map((r) => {
                const pts = roadPoints(map, r).map(([x, y]) => `${x},${y}`).join(' ');
                const mid = roadMiddle(map, r);
                return (
                  <g key={`${r.a}-${r.b}`}>
                    <polyline points={pts} fill="none" stroke="transparent" strokeWidth={14} className="preview-road" onClick={() => tool === 'delete' && setMap(deleteRoad(map, r.a, r.b))} />
                    <polyline points={pts} fill="none" stroke="#e8dcc0" strokeWidth={3} pointerEvents="none" />
                    {tool === 'reshape' && mid && (
                      <rect
                        x={mid[0] - 7} y={mid[1] - 7} width={14} height={14} rx={3} className="road-handle"
                        onPointerDown={(e) => {
                          e.stopPropagation();
                          (e.currentTarget.ownerSVGElement as SVGSVGElement).setPointerCapture(e.pointerId);
                          dragging.current = { a: r.a, b: r.b };
                        }}
                      />
                    )}
                  </g>
                );
              })}
              {map.places.map((p) => (
                <g key={p.id} className="map-place" onClick={(e) => { e.stopPropagation(); onPlace(p.id); }}>
                  <circle cx={p.x} cy={p.y} r={p.capital ? 11 : 8} fill={p.id === selected || p.id === roadStart ? '#f0b232' : '#5aa9e6'} stroke="#111214" strokeWidth={2} />
                  <text x={p.x} y={p.y - 14} textAnchor="middle" className="map-label" fontSize={18}>{p.name}</text>
                </g>
              ))}
            </svg>
          )
        )}
      </div>
      <aside className="import-settings">
        <fieldset>
          <legend>{t('activity_ui__generate_settings')}</legend>
          <label>
            {t('activity_ui__generate_count')}
            <input type="number" min={MIN_PLACES} max={MAX_PLACES} value={count} onChange={(e) => setCount(Math.max(MIN_PLACES, Math.min(MAX_PLACES, Number(e.target.value) || MIN_PLACES)))} />
          </label>
          <label>
            {t('activity_ui__generate_seed')}
            <input type="number" min={0} max={4294967295} value={seed} onChange={(e) => setSeed(Math.max(0, Math.min(4294967295, Math.floor(Number(e.target.value) || 0))))} />
          </label>
          <div className="import-nav">
            <button type="button" className="secondary small" disabled={busy} onClick={() => void generate(seed)}>{t('activity_ui__generate_apply')}</button>
            <button type="button" className="secondary small" disabled={busy} onClick={() => void generate(randomSeed())}>{t('activity_ui__generate_new_seed')}</button>
          </div>
          <p className="muted small-text">{t('activity_ui__generate_regenerate_hint')}</p>
        </fieldset>
        <fieldset>
          <legend>{t('activity_ui__generate_tools')}</legend>
          {(Object.keys(TOOL_LABEL) as Tool[]).map((k) => (
            <label key={k}>
              <input type="radio" name="tool" checked={tool === k} disabled={view3d} onChange={() => { setTool(k); setRoadStart(null); }} />
              {t(TOOL_LABEL[k])}
            </label>
          ))}
          {tool === 'add_road' && <p className="muted small-text">{t(roadStart === null ? 'activity_ui__generate_road_first' : 'activity_ui__generate_road_second')}</p>}
          {can3d && (
            <label>
              <input type="checkbox" checked={view3d} onChange={(e) => setView3d(e.target.checked)} />
              {t('activity_ui__generate_relief_view')}
            </label>
          )}
        </fieldset>
        {place && tool === 'select' && (
          <fieldset>
            <legend>{t('activity_ui__generate_place')}</legend>
            <label>
              {t('activity_ui__import_universe_name')}
              <input value={place.name} maxLength={100} onChange={(e) => setMap((m) => (m ? renamePlace(m, place.id, e.target.value) : m))} />
            </label>
          </fieldset>
        )}
        {map && <p className="muted small-text">{t('activity_ui__generate_counts', { places: map.places.length, roads: map.roads.length })}</p>}
        {error && <p className="import-error">{t(error)}</p>}
        <div className="import-nav">
          <button type="button" className="secondary" onClick={onBack}>{t('activity_ui__back')}</button>
          <button type="button" disabled={busy || !!problem} title={problem ? t(problem) : undefined} onClick={() => void useMap()}>{t('activity_ui__generate_use')}</button>
        </div>
      </aside>
    </div>
  );
}
