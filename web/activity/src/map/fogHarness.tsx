// Page de développement (`/fog-harness.html?view=3d|2d`) : la carte sous brouillard, sans Discord.
import { lazy, Suspense, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import '../styles.css';
import type { MapView } from '../api';
import { MapView2D } from '../components/MapView2D';

const MapScene = lazy(() => import('../components/MapScene').then((m) => ({ default: m.MapScene })));

function Harness() {
  const [map, setMap] = useState<MapView | null>(null);
  const view = new URLSearchParams(location.search).get('view') ?? '3d';
  useEffect(() => {
    void fetch('/fixture/fog-map.json').then((r) => r.json()).then(setMap);
  }, []);
  if (!map) return null;
  const props = { map, currentPlaceId: map.fog?.discovered[0] ?? null, reachable: new Set(map.fog?.glimpsed ?? []), motion: null, playerPicture: null, playerName: 'Testeur', onSelectPlace: () => {} };
  return (
    <div className="map-view" style={{ height: '100%' }}>
      {view === '2d' ? <MapView2D {...props} /> : <Suspense fallback={null}><MapScene {...props} /></Suspense>}
    </div>
  );
}

createRoot(document.getElementById('root')!).render(<Harness />);
