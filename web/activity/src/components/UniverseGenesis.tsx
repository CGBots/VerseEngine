import { useEffect, useRef, useState } from 'react';
import type { Translator } from '../i18n';

/**
 * Naissance d'un univers, jouée à la fin d'un import :
 * 1. la commande `/univers nouvel_univers nom: …` se tape dans une barre de message façon Discord ;
 * 2. « Entrée » : la barre se resserre en un point de lumière ;
 * 3. big bang : le point explose en blanc et couvre l'écran ;
 * 4. le blanc se dissipe et révèle la carte, chargée derrière pendant ce temps.
 *
 * Le blanc tient tant que la carte n'est pas prête (8 s au plus). Un clic passe l'animation.
 * Avec `prefers-reduced-motion`, la frappe et l'explosion sont sautées : écran blanc avec le nom,
 * puis la carte.
 */
type Phase = 'typing' | 'enter' | 'bang' | 'white' | 'reveal';

const TYPE_MS = 42;
const ENTER_MS = 520;
const BANG_MS = 900;
const MIN_WHITE_MS = 700;
const MAX_WHITE_MS = 8000;
const REVEAL_MS = 1400;

interface Props {
  t: Translator;
  universeName: string;
  /** La carte de l'univers est chargée : le blanc peut se dissiper. */
  mapReady: boolean;
  onFinished: () => void;
}

/** Nom localisé d'une commande ou d'une option, ou le nom interne si la traduction manque. */
const label = (t: Translator, key: string, fallback: string) => {
  const v = t(key);
  return v === key ? fallback : v;
};

export function UniverseGenesis({ t, universeName, mapReady, onFinished }: Props) {
  const reduced = useRef(typeof window !== 'undefined' && window.matchMedia?.('(prefers-reduced-motion: reduce)').matches).current;
  const command = `/${label(t, 'universe', 'universe')} ${label(t, 'universe_create_universe', 'create_universe')}`;
  const option = label(t, 'universe_create_universe.universe_name', 'name');
  const full = command.length + universeName.length;

  const [phase, setPhase] = useState<Phase>(reduced ? 'white' : 'typing');
  const [typed, setTyped] = useState(reduced ? full : 0);
  const whiteSince = useRef<number | null>(reduced ? performance.now() : null);
  const finished = useRef(false);

  // 1. Frappe, caractère par caractère (rythme légèrement irrégulier, comme une vraie frappe).
  useEffect(() => {
    if (phase !== 'typing') return;
    if (typed >= full) {
      const id = window.setTimeout(() => setPhase('enter'), 380);
      return () => window.clearTimeout(id);
    }
    // Petite pause entre la commande et le nom, comme quand Discord ouvre l'option.
    const pause = typed === command.length ? 260 : 0;
    const id = window.setTimeout(() => setTyped((n) => n + 1), TYPE_MS + Math.random() * 40 + pause);
    return () => window.clearTimeout(id);
  }, [phase, typed, full, command.length]);

  // 2 et 3. Entrée, puis explosion.
  useEffect(() => {
    if (phase === 'enter') {
      const id = window.setTimeout(() => setPhase('bang'), ENTER_MS);
      return () => window.clearTimeout(id);
    }
    if (phase === 'bang') {
      const id = window.setTimeout(() => {
        whiteSince.current = performance.now();
        setPhase('white');
      }, BANG_MS);
      return () => window.clearTimeout(id);
    }
  }, [phase]);

  // 4. Le blanc se dissipe dès que la carte est prête (après un court temps de pose), ou au plus tard.
  useEffect(() => {
    if (phase !== 'white') return;
    const since = whiteSince.current ?? performance.now();
    const elapsed = performance.now() - since;
    const wait = mapReady ? Math.max(0, MIN_WHITE_MS - elapsed) : Math.max(0, MAX_WHITE_MS - elapsed);
    const id = window.setTimeout(() => setPhase('reveal'), wait);
    return () => window.clearTimeout(id);
  }, [phase, mapReady]);

  useEffect(() => {
    if (phase !== 'reveal') return;
    const id = window.setTimeout(() => {
      if (!finished.current) {
        finished.current = true;
        onFinished();
      }
    }, REVEAL_MS);
    return () => window.clearTimeout(id);
  }, [phase, onFinished]);

  const skip = () => {
    if (phase === 'typing' || phase === 'enter' || phase === 'bang') {
      setTyped(full);
      whiteSince.current = performance.now();
      setPhase('white');
    } else if (phase === 'white' && mapReady) {
      setPhase('reveal');
    }
  };

  const commandPart = command.slice(0, Math.min(typed, command.length));
  const namePart = universeName.slice(0, Math.max(0, typed - command.length));
  const showOption = typed >= command.length;

  return (
    <div className={`genesis genesis-${phase}${reduced ? ' genesis-reduced' : ''}`} onClick={skip} role="presentation">
      <div className="genesis-space" aria-hidden="true" />
      <div className="genesis-input" aria-hidden={phase !== 'typing'}>
        <span className="genesis-command">{commandPart}</span>
        {showOption && (
          <>
            {' '}
            <span className="genesis-option">{option}</span>
            <span className="genesis-value">{namePart}</span>
          </>
        )}
        <span className="genesis-caret" />
      </div>
      <div className="genesis-flash" aria-hidden="true" />
      <div className="genesis-shockwave" aria-hidden="true" />
      <p className="genesis-title" aria-live="polite">{phase === 'white' || phase === 'reveal' ? universeName : ''}</p>
    </div>
  );
}
