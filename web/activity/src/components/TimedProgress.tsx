import { useEffect, useState } from 'react';
import { clock } from '../api';
import { progress } from '../occupation';

/** Barre de progression entre deux instants serveur, mise à jour à 1 Hz (comme `Countdown`). */
export function TimedProgress({ start, end, label, onElapsed }: { start: number; end: number; label: string; onElapsed?: () => void }) {
  const [now, setNow] = useState(() => clock.now());
  useEffect(() => {
    setNow(clock.now());
    const id = window.setInterval(() => {
      const n = clock.now();
      setNow(n);
      if (n >= end) {
        window.clearInterval(id);
        onElapsed?.();
      }
    }, 1000);
    return () => window.clearInterval(id);
  }, [end, onElapsed]);
  const pct = Math.round(progress(start, end, now) * 100);
  return (
    <div className="progress" role="progressbar" aria-label={label} aria-valuemin={0} aria-valuemax={100} aria-valuenow={pct}>
      <div className="progress-bar" style={{ width: `${pct}%` }} />
    </div>
  );
}
