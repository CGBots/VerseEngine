// Page de développement (`/import-harness.html`) : l'assistant d'import sans Discord. L'API est
// simulée par le test du navigateur (interception des requêtes `/api/*`).
import { StrictMode, useEffect, useState } from 'react';
import { createRoot } from 'react-dom/client';
import '../styles.css';
import { loadTranslator, type Translator } from '../i18n';
import { ImportWizard } from './ImportWizard';

function Harness() {
  const [t, setT] = useState<Translator | null>(null);
  const [done, setDone] = useState(false);
  const params = new URLSearchParams(location.search);
  useEffect(() => {
    void loadTranslator(params.get('locale') === 'en' ? 'en-US' : 'fr').then((tr) => setT(() => tr));
  }, []);
  if (!t) return null;
  if (done) return <p className="center">done</p>;
  return (
    <div className="import-page">
      <ImportWizard t={t} universeName={params.get('universe')} serversVersion={0} onDone={() => setDone(true)} />
    </div>
  );
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Harness />
  </StrictMode>,
);
