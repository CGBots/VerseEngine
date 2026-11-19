import { describe, expect, it } from 'vitest';
import { formatClock, timeOfDay } from './universeClock';

// Mêmes cas que les tests Rust de `universe::time::time_of_day`.
describe('timeOfDay', () => {
  const real = { origin: 1000, phase_duration_secs: 21_600 };
  it('commence la journée à minuit', () => {
    expect(timeOfDay(1000, real)).toEqual({ hours: 0, minutes: 0, phase: 'midnight' });
    expect(timeOfDay(1000 + 6 * 3600, real)).toEqual({ hours: 6, minutes: 0, phase: 'sunrise' });
    expect(timeOfDay(1000 + 13 * 3600 + 30 * 60, real)).toEqual({ hours: 13, minutes: 30, phase: 'noon' });
    expect(timeOfDay(1000 + 23 * 3600 + 59 * 60, real)).toEqual({ hours: 23, minutes: 59, phase: 'sunset' });
    expect(timeOfDay(1000 + 24 * 3600, real)).toEqual({ hours: 0, minutes: 0, phase: 'midnight' });
  });
  it('tourne plus vite dans un univers accéléré', () => {
    expect(timeOfDay(3 * 3600, { origin: 0, phase_duration_secs: 10_800 })).toEqual({ hours: 6, minutes: 0, phase: 'sunrise' });
  });
  it("vaut minuit avant l'origine", () => {
    expect(timeOfDay(10, real)).toEqual({ hours: 0, minutes: 0, phase: 'midnight' });
  });
  it('formate HH:MM', () => {
    expect(formatClock(7, 5)).toBe('07:05');
  });
});
