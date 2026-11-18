import { useCallback, useEffect, useState } from 'react';
import { api, ApiError, type CharacterView, type SessionInfo } from './api';
import { connect } from './discord';
import { loadTranslator, pickLocale, type Translator } from './i18n';
import { CharacterPanel } from './components/CharacterPanel';

type State =
  | { kind: 'loading' }
  | { kind: 'error'; key: string }
  | { kind: 'ready'; session: SessionInfo; character: CharacterView | null; characterError?: string };

const fallbackT: Translator = (k) => k;

export function App() {
  const [state, setState] = useState<State>({ kind: 'loading' });
  const [t, setT] = useState<Translator>(() => fallbackT);
  const [adminMode, setAdminMode] = useState(false);

  const loadCharacter = useCallback(async (session: SessionInfo) => {
    try {
      const character = await api<CharacterView>('/me/character');
      setState({ kind: 'ready', session, character });
    } catch (e) {
      setState({ kind: 'ready', session, character: null, characterError: e instanceof ApiError ? e.key : 'activity__internal_error' });
    }
  }, []);

  const start = useCallback(async () => {
    setState({ kind: 'loading' });
    try {
      const { session, locale } = await connect();
      setT(() => fallbackT);
      const translator = await loadTranslator(pickLocale(locale));
      setT(() => translator);
      await loadCharacter(session);
    } catch (e) {
      setState({ kind: 'error', key: e instanceof ApiError ? e.key : 'activity__unauthorized' });
    }
  }, [loadCharacter]);

  useEffect(() => {
    void start();
  }, [start]);

  if (state.kind === 'loading') return <div className="center">{t('activity_ui__loading')}</div>;
  if (state.kind === 'error')
    return (
      <div className="center">
        <p>{t(state.key)}</p>
        <button type="button" onClick={() => void start()}>{t('activity_ui__retry')}</button>
      </div>
    );

  const { session, character } = state;
  const avatar = session.user.avatar ? `https://cdn.discordapp.com/avatars/${session.user.id}/${session.user.avatar}.png?size=128` : null;
  return (
    <div className="layout">
      {character ? (
        <CharacterPanel
          character={character}
          avatarUrl={avatar}
          t={t}
          onOpenInventory={() => { /* phase c */ }}
          onResync={() => void loadCharacter(session)}
        />
      ) : (
        <aside className="side-panel"><p>{t(state.characterError ?? 'activity__no_character')}</p></aside>
      )}
      <main className="map-area">
        <div className="toolbar">
          <span>{session.universe.name}</span>
          {session.admin_mode_available && (
            <label className="admin-toggle">
              <input type="checkbox" checked={adminMode} onChange={(e) => setAdminMode(e.target.checked)} />
              {t('activity_ui__admin_mode')}
            </label>
          )}
        </div>
        <div className="map-placeholder">{t('activity_ui__map')}</div>
      </main>
    </div>
  );
}
