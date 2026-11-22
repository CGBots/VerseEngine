// Page de développement (`/music-harness.html?view=player|admin`) : la musique sans Discord.
// L'API est simulée par le test du navigateur (interception des requêtes `/api/*`).
import { StrictMode, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import '../styles.css';
import { loadTranslator, type Translator } from '../i18n';
import { MusicControl } from '../components/MusicControl';

function Harness() {
  const [t, setT] = useState<Translator | null>(null);
  const [version, setVersion] = useState(0);
  useEffect(() => {
    void loadTranslator('fr').then((tr) => setT(() => tr));
  }, []);
  if (!t) return null;
  return (
    <div className="map-view" style={{ height: '100vh' }}>
      <button type="button" onClick={() => setVersion((v) => v + 1)}>Déplacer</button>
      <MusicControl t={t} version={version} />
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Harness />
  </StrictMode>,
);
