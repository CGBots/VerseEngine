// Page de développement (`/app-harness.html`) : l'activité entière sans Discord, pour vérifier la
// mise en page sur différents écrans. L'API est simulée par le test du navigateur (`/api/*`).
import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import type { DiscordSDK } from '@discord/embedded-app-sdk';
import { App } from './App';
import { api, setSessionToken, type SessionInfo } from './api';
import './styles.css';

async function fakeConnect() {
  const session = await api<SessionInfo>('/auth/token', { method: 'POST', body: '{}' });
  setSessionToken(session.session_token);
  return { sdk: null as unknown as DiscordSDK, session, locale: 'fr' };
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App connectFn={fakeConnect} />
  </StrictMode>,
);
