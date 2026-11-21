import { useEffect, useRef, useState } from 'react';
import { openExternal } from '../discord';
import type { Translator } from '../i18n';
import { AZGAAR_PATH, AZGAAR_SANDBOX, exportRequest, mapBytes, parseAzgaarMessage } from './azgaarBridge';

const AZGAAR_URL = 'https://azgaar.github.io/Fantasy-Map-Generator/';
/** Sans carte prête après ce délai, Azgaar est considéré comme indisponible ici. */
const READY_TIMEOUT_MS = 90_000;
/** Délai de réponse à une demande de carte. */
const EXPORT_TIMEOUT_MS = 30_000;

interface Props {
  t: Translator;
  /** Masqué (mais gardé chargé) quand l'assistant passe à l'étape suivante. */
  hidden: boolean;
  busy: boolean;
  /** Carte reçue d'Azgaar, déjà vérifiée (taille) ; elle part ensuite comme un `.map` choisi à la main. */
  onMap: (bytes: Uint8Array) => void;
  onBack: () => void;
  onError: (key: string) => void;
}

/** Azgaar dans l'activité (E9), dans un cadre isolé : voir `azgaarBridge.ts`. */
export function AzgaarStudio({ t, hidden, busy, onMap, onBack, onError }: Props) {
  const frame = useRef<HTMLIFrameElement>(null);
  const [ready, setReady] = useState(false);
  const [unavailable, setUnavailable] = useState(false);
  const [pending, setPending] = useState<number | null>(null);
  const pendingRef = useRef<number | null>(null);
  const nextId = useRef(1);
  const callbacks = useRef({ onMap, onError });
  callbacks.current = { onMap, onError };

  useEffect(() => {
    const onMessage = (event: MessageEvent) => {
      const m = parseAzgaarMessage(event, frame.current?.contentWindow ?? null);
      if (!m) return;
      if (m.kind === 'ready') {
        setReady(true);
        setUnavailable(false);
        return;
      }
      // Seule la réponse à la demande en cours compte.
      if (pendingRef.current !== m.id) return;
      pendingRef.current = null;
      setPending(null);
      if (m.kind === 'error') callbacks.current.onError('activity_ui__azgaar_failed');
      else {
        const bytes = mapBytes(m.data);
        if (bytes) callbacks.current.onMap(bytes);
        else callbacks.current.onError('activity__payload_too_large');
      }
    };
    window.addEventListener('message', onMessage);
    return () => window.removeEventListener('message', onMessage);
  }, []);

  useEffect(() => {
    if (ready) return;
    const id = window.setTimeout(() => setUnavailable(true), READY_TIMEOUT_MS);
    return () => window.clearTimeout(id);
  }, [ready]);

  useEffect(() => {
    if (pending === null) return;
    const id = window.setTimeout(() => {
      pendingRef.current = null;
      setPending(null);
      callbacks.current.onError('activity_ui__azgaar_failed');
    }, EXPORT_TIMEOUT_MS);
    return () => window.clearTimeout(id);
  }, [pending]);

  const send = () => {
    const win = frame.current?.contentWindow;
    if (!win) return;
    const id = nextId.current++;
    pendingRef.current = id;
    setPending(id);
    // Origine opaque : on ne peut pas la nommer. La demande ne contient rien de sensible.
    win.postMessage(exportRequest(id), '*');
  };

  return (
    <div className="azgaar-studio" hidden={hidden}>
      <p className="azgaar-hint">{t('activity_ui__azgaar_hint')}</p>
      {unavailable && (
        <p className="import-error" role="alert">
          {t('activity_ui__azgaar_unavailable')}{' '}
          <button type="button" className="secondary small" onClick={() => void openExternal(AZGAAR_URL)}>azgaar.github.io</button>
        </p>
      )}
      <iframe ref={frame} className="azgaar-frame" src={AZGAAR_PATH} sandbox={AZGAAR_SANDBOX} title="Azgaar's Fantasy Map Generator" />
      <div className="import-nav">
        <button type="button" className="secondary" onClick={onBack}>{t('activity_ui__back')}</button>
        <button type="button" disabled={!ready || busy || pending !== null} onClick={send}>
          {!ready ? t('activity_ui__azgaar_loading') : busy || pending !== null ? t('activity_ui__azgaar_sending') : t('activity_ui__azgaar_send')}
        </button>
      </div>
    </div>
  );
}
