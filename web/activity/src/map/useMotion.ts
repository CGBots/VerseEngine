import { useEffect, useRef, useState } from 'react';
import { clock } from '../api';
import { pointAlong, roadProgress, smoothToward, type MapPoint, type PlayerMotion } from './geometry';

export interface AnimatedPosition {
  at: MapPoint;
  /** Avancement sur la route (0–1), `null` dans un lieu. */
  progress: number | null;
}

const reducedMotion = () => typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches;

/**
 * Position animée du joueur : extrapolée en continu à sa vitesse (≈ 30 images/s pendant un
 * voyage, 1/s si l'utilisateur limite les animations), et lissée quand le serveur la corrige.
 * Ne provoque de rendu que dans le composant de carte qui l'utilise.
 */
export function useAnimatedPosition(motion: PlayerMotion | null): AnimatedPosition | null {
  const [pos, setPos] = useState<AnimatedPosition | null>(null);
  const shown = useRef<number>(NaN);
  const roadKey = useRef<string>('');

  useEffect(() => {
    if (!motion) return setPos(null);
    if (motion.kind === 'place') {
      shown.current = NaN;
      return setPos({ at: motion.at, progress: null });
    }
    // Nouvelle route (ou demi-tour) : pas de glissement depuis l'ancienne position.
    const key = `${motion.from.x},${motion.from.y}>${motion.to.x},${motion.to.y}`;
    if (key !== roadKey.current) {
      roadKey.current = key;
      shown.current = NaN;
    }
    const reduce = reducedMotion();
    let raf = 0;
    let timer = 0;
    let last = performance.now();
    const step = () => {
      const t = performance.now();
      const target = roadProgress(motion, clock.now());
      shown.current = reduce ? target : smoothToward(shown.current, target, (t - last) / 1000);
      last = t;
      setPos({ at: pointAlong(motion.path, shown.current), progress: shown.current });
    };
    step();
    if (reduce) {
      timer = window.setInterval(step, 1000);
    } else {
      let previous = 0;
      const loop = (t: number) => {
        if (t - previous >= 33) {
          previous = t;
          step();
        }
        raf = requestAnimationFrame(loop);
      };
      raf = requestAnimationFrame(loop);
    }
    return () => {
      cancelAnimationFrame(raf);
      window.clearInterval(timer);
    };
  }, [motion]);

  return pos;
}
