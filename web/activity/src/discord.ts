import { DiscordSDK } from '@discord/embedded-app-sdk';
import { api, setSessionToken, type SessionInfo } from './api';

const CLIENT_ID = import.meta.env.VITE_DISCORD_CLIENT_ID;

/**
 * Flux OAuth2 de l'Embedded App SDK :
 * authorize (code) → échange côté serveur (secret) → authenticate (access_token).
 * Le serveur vérifie lui-même l'instance, la guilde et les droits.
 */
export async function connect(): Promise<{ sdk: DiscordSDK; session: SessionInfo; locale?: string }> {
  if (!CLIENT_ID) throw new Error('VITE_DISCORD_CLIENT_ID manquant');
  const sdk = new DiscordSDK(CLIENT_ID);
  const instanceId = sdk.instanceId;
  await sdk.ready();

  const { code } = await sdk.commands.authorize({
    client_id: CLIENT_ID,
    response_type: 'code',
    state: '',
    prompt: 'none',
    scope: ['identify'],
  });

  const session = await api<SessionInfo>('/auth/token', {
    method: 'POST',
    body: JSON.stringify({ code, instance_id: instanceId }),
  });
  setSessionToken(session.session_token);
  await sdk.commands.authenticate({ access_token: session.access_token });

  let locale: string | undefined;
  try {
    locale = (await sdk.commands.userSettingsGetLocale()).locale;
  } catch {
    locale = navigator.language;
  }
  return { sdk, session, locale };
}
