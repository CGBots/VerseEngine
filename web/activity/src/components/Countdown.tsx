import { useEffect, useState } from 'react';
import { clock } from '../api';
import { formatDuration } from '../time';

/** Compte à rebours DOM à 1 Hz, calculé localement à partir d'un timestamp serveur. */
export function Countdown({ until, onElapsed }: { until: number; onElapsed?: () => void }) {
  const [remaining, setRemaining] = useState(() => clock.remaining(until));
  useEffect(() => {
    setRemaining(clock.remaining(until));
    const id = window.setInterval(() => {
      const r = clock.remaining(until);
      setRemaining(r);
      if (r === 0) {
        window.clearInterval(id);
        onElapsed?.();
      }
    }, 1000);
    return () => window.clearInterval(id);
  }, [until, onElapsed]);
  return <span className="countdown" aria-live="polite">{formatDuration(remaining)}</span>;
}
