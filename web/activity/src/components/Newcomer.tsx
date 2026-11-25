import { useState } from 'react';
import { ApiError, newcomerApi } from '../api';
import { openExternal } from '../discord';
import type { Translator } from '../i18n';

const MAX_NAME = 32;
const MAX_TEXT = 1024;
const len = (s: string) => [...s.trim()].length;

interface Props {
  t: Translator;
  spectator: boolean;
  onSpectate: (on: boolean) => void;
}

/** Joueur sans personnage dans l'univers : demander un personnage (fiche envoyée aux modérateurs,
 *  comme `/character_create`) ou regarder en spectateur (carte dévoilée, tous les groupes). */
export function Newcomer({ t, spectator, onSpectate }: Props) {
  const [form, setForm] = useState<null | { name: string; description: string; story: string; special_request: string }>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);
  const [sentUrl, setSentUrl] = useState<string | null>(null);

  const submit = async () => {
    if (!form) return;
    setBusy(true);
    setMessage(null);
    try {
      const r = await newcomerApi.requestCharacter(form);
      setSentUrl(r.url);
      setForm(null);
      setMessage(t(r.result));
    } catch (e) {
      setMessage(t(e instanceof ApiError ? e.key : 'activity__internal_error'));
    } finally {
      setBusy(false);
    }
  };

  if (form) {
    const field = (key: keyof typeof form, label: string, max: number, rows = 0, required = true) => (
      <label className="newcomer-field">
        <span className="label">{t(label)}{!required && <span className="muted"> · {t('activity_ui__optional')}</span>}</span>
        {rows ? (
          <textarea rows={rows} value={form[key]} maxLength={max} onChange={(e) => setForm({ ...form, [key]: e.target.value })} />
        ) : (
          <input value={form[key]} maxLength={max} onChange={(e) => setForm({ ...form, [key]: e.target.value })} />
        )}
        <span className="muted counter">{len(form[key])} / {max}</span>
      </label>
    );
    const ok = len(form.name) > 0 && len(form.description) > 0 && len(form.story) > 0;
    return (
      <form className="newcomer" onSubmit={(e) => { e.preventDefault(); void submit(); }}>
        <h2>{t('activity_ui__character_create')}</h2>
        <p className="muted">{t('activity_ui__character_create_hint')}</p>
        {field('name', 'activity_ui__character_name', MAX_NAME)}
        {field('description', 'activity_ui__character_description', MAX_TEXT, 4)}
        {field('story', 'activity_ui__character_story', MAX_TEXT, 5)}
        {field('special_request', 'activity_ui__character_special_request', MAX_TEXT, 3, false)}
        {message && <p className="action-message" role="status">{message}</p>}
        <div className="row">
          <button type="submit" disabled={busy || !ok}>{t('activity_ui__character_send')}</button>
          <button type="button" className="secondary" disabled={busy} onClick={() => setForm(null)}>{t('activity_ui__cancel')}</button>
        </div>
      </form>
    );
  }

  return (
    <div className="newcomer">
      <p className="side-panel-message">{t(spectator ? 'activity_ui__spectating' : 'activity__no_character')}</p>
      {message && <p className="action-message" role="status">{message}</p>}
      {sentUrl && (
        <a href={sentUrl} onClick={(e) => { e.preventDefault(); void openExternal(sentUrl); }}>{t('activity_ui__character_request_open')}</a>
      )}
      <nav className="actions">
        {!sentUrl && (
          <button type="button" className="action" onClick={() => { setMessage(null); setForm({ name: '', description: '', story: '', special_request: '' }); }}>
            {t('activity_ui__character_create')}
          </button>
        )}
        <button type="button" className="action secondary" aria-pressed={spectator} onClick={() => onSpectate(!spectator)}>
          {t(spectator ? 'activity_ui__spectator_leave' : 'activity_ui__spectator')}
        </button>
      </nav>
    </div>
  );
}
