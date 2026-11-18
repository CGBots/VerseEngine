//! Minuteur unique par file d'attente (voyages, crafts, loots).
//!
//! Chaque file (voyage, craft, loot) n'a besoin que d'un réveil : l'instant où son premier élément
//! arrive à échéance. Ce module garantit :
//!
//! - **un seul minuteur vivant** : chaque (re)programmation incrémente une génération et arrête
//!   (`abort`) le réveil précédent ; un réveil d'une génération dépassée ne fait rien. C'est la
//!   méthode du cycle jour/nuit, appliquée ici aux `setup()` rappelés à chaque `ready` Discord ;
//! - **un traitement jamais interrompu** : le travail (finaliser un craft, faire arriver un
//!   groupe…) tourne dans une tâche détachée. Un `abort` du réveil ne coupe donc jamais une
//!   transaction ou une mise à jour de rôles en cours ;
//! - **un traitement à la fois** : le travail s'exécute sous un verrou que `setup()` prend aussi
//!   ([`Timer::exclusive`]) avant de relire la base, pour ne pas finaliser deux fois un élément.

use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::Duration;

use tokio::sync::{Mutex, MutexGuard};
use tokio::task::JoinHandle;
use tokio::time::Instant;

/// Travail exécuté au réveil. Il traite les éléments échus et retourne le délai (en secondes)
/// avant le prochain réveil, ou `None` s'il n'y a plus rien à attendre.
pub type Job = Arc<dyn Fn() -> Pin<Box<dyn Future<Output = Option<u64>> + Send>> + Send + Sync>;

/// Construit un [`Job`] à partir d'une fonction asynchrone sans argument.
pub fn job<F, Fut>(f: F) -> Job
where
    F: Fn() -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Option<u64>> + Send + 'static,
{
    Arc::new(move || Box::pin(f()))
}

/// Vérifie qu'un élément a bien été retiré de la base par ce traitement-ci.
///
/// La finalisation d'un craft ou d'un loot supprime son document dans la même transaction que la
/// remise des objets. Si rien n'a été supprimé, l'élément a déjà été finalisé (ou annulé par le
/// joueur) ailleurs : la transaction doit être annulée pour ne pas donner les objets deux fois.
pub fn ensure_claimed(deleted_count: u64) -> Result<(), &'static str> {
    if deleted_count == 0 { Err("already finalized or cancelled") } else { Ok(()) }
}

struct State {
    generation: u64,
    /// Échéance du réveil programmé, `None` si aucun réveil n'est prévu.
    deadline: Option<Instant>,
    handle: Option<JoinHandle<()>>,
}

pub struct Timer {
    name: &'static str,
    state: StdMutex<State>,
    processing: Mutex<()>,
}

impl Timer {
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            state: StdMutex::new(State { generation: 0, deadline: None, handle: None }),
            processing: Mutex::new(()),
        }
    }

    fn lock_state(&self) -> std::sync::MutexGuard<'_, State> {
        // Un panic pendant qu'on tenait ce verrou (jamais à travers un `await`) ne doit pas
        // bloquer le minuteur pour toujours.
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Génération courante (incrémentée à chaque programmation ou arrêt).
    pub fn generation(&self) -> u64 {
        self.lock_state().generation
    }

    /// Vrai si un réveil est programmé.
    pub fn is_armed(&self) -> bool {
        self.lock_state().deadline.is_some()
    }

    /// Programme un réveil dans `delay` secondes, **à la place** de celui en cours (arrêté).
    /// `None` arrête simplement le minuteur. À utiliser par `setup()`.
    pub fn restart(&'static self, delay: Option<u64>, job: Job) {
        let mut state = self.lock_state();
        self.replace(&mut state, delay, job);
    }

    /// Programme un réveil dans `delay` secondes s'il n'y en a aucun ou si celui en cours est
    /// plus tardif. Un réveil plus proche déjà prévu est conservé. À utiliser quand un élément
    /// est ajouté à une file.
    pub fn schedule(&'static self, delay: u64, job: Job) {
        let mut state = self.lock_state();
        let wanted = Instant::now() + Duration::from_secs(delay);
        if state.deadline.is_some_and(|current| current <= wanted) {
            return;
        }
        self.replace(&mut state, Some(delay), job);
    }

    /// Arrête le minuteur : le réveil prévu est annulé (un traitement en cours se termine).
    pub fn stop(&'static self) {
        let mut state = self.lock_state();
        state.generation += 1;
        state.deadline = None;
        if let Some(handle) = state.handle.take() {
            handle.abort();
        }
    }

    /// Attend la fin du traitement en cours et empêche tout autre traitement tant que le verrou
    /// retourné est gardé. `setup()` le prend avant de relire la base.
    pub async fn exclusive(&self) -> MutexGuard<'_, ()> {
        self.processing.lock().await
    }

    fn replace(&'static self, state: &mut State, delay: Option<u64>, job: Job) {
        state.generation += 1;
        if let Some(handle) = state.handle.take() {
            handle.abort();
        }
        match delay {
            Some(delay) => {
                let deadline = Instant::now() + Duration::from_secs(delay);
                state.deadline = Some(deadline);
                state.handle = Some(tokio::spawn(self.run(state.generation, deadline, job)));
            }
            None => state.deadline = None,
        }
    }

    fn is_current(&self, generation: u64) -> bool {
        self.lock_state().generation == generation
    }

    async fn run(&'static self, generation: u64, mut deadline: Instant, job: Job) {
        loop {
            tokio::time::sleep_until(deadline).await;
            if !self.is_current(generation) {
                return;
            }

            // Tâche détachée : un `abort` de ce réveil (nouvelle programmation) ne l'interrompt pas.
            let work = job.clone();
            let next = tokio::spawn(async move {
                let _guard = self.processing.lock().await;
                // Un `setup()` a pu tout reprendre pendant l'attente du verrou.
                if !self.is_current(generation) {
                    return None;
                }
                work().await
            })
            .await
            .unwrap_or_else(|e| {
                eprintln!("[timer:{}] the job failed: {e:?}", self.name);
                None
            });

            let mut state = self.lock_state();
            if state.generation != generation {
                // Une programmation plus récente existe. On ne fait qu'avancer son échéance si le
                // traitement qu'on vient de faire a besoin d'un réveil plus tôt.
                drop(state);
                if let Some(delay) = next {
                    self.schedule(delay, job);
                }
                return;
            }
            match next {
                Some(delay) => {
                    deadline = Instant::now() + Duration::from_secs(delay);
                    state.deadline = Some(deadline);
                }
                None => {
                    state.deadline = None;
                    state.handle = None;
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    #[test]
    fn a_finalization_must_remove_the_document() {
        assert!(ensure_claimed(1).is_ok());
        assert!(ensure_claimed(0).is_err(), "nothing removed: already finalized elsewhere");
    }

    fn leak(name: &'static str) -> &'static Timer {
        Box::leak(Box::new(Timer::new(name)))
    }

    /// Job qui compte ses exécutions et ne reprogramme rien.
    fn counting_job(count: Arc<AtomicUsize>) -> Job {
        job(move || {
            let count = count.clone();
            async move {
                count.fetch_add(1, Ordering::SeqCst);
                None
            }
        })
    }

    async fn advance(secs: u64) {
        tokio::time::sleep(Duration::from_secs(secs)).await;
        // Laisse les tâches réveillées finir leur travail.
        for _ in 0..10 {
            tokio::task::yield_now().await;
        }
    }

    #[tokio::test(start_paused = true)]
    async fn the_job_runs_once_at_the_deadline() {
        let timer = leak("t");
        let count = Arc::new(AtomicUsize::new(0));
        timer.restart(Some(10), counting_job(count.clone()));
        advance(9).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
        advance(2).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(!timer.is_armed());
    }

    #[tokio::test(start_paused = true)]
    async fn repeated_setups_leave_a_single_timer() {
        // Trois `ready` Discord de suite (reconnexions) : un seul traitement à l'échéance.
        let timer = leak("t");
        let count = Arc::new(AtomicUsize::new(0));
        for _ in 0..3 {
            timer.restart(Some(10), counting_job(count.clone()));
        }
        advance(30).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn schedule_keeps_an_earlier_wake_up() {
        let timer = leak("t");
        let count = Arc::new(AtomicUsize::new(0));
        timer.schedule(10, counting_job(count.clone()));
        timer.schedule(100, counting_job(count.clone())); // plus tard : ignoré
        advance(11).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "the earlier wake-up is kept");
    }

    #[tokio::test(start_paused = true)]
    async fn schedule_brings_a_later_wake_up_forward() {
        let timer = leak("t");
        let count = Arc::new(AtomicUsize::new(0));
        timer.schedule(100, counting_job(count.clone()));
        timer.schedule(10, counting_job(count.clone()));
        advance(11).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
        advance(100).await;
        assert_eq!(count.load(Ordering::SeqCst), 1, "the replaced wake-up never fires");
    }

    #[tokio::test(start_paused = true)]
    async fn the_job_delay_rearms_the_timer() {
        let timer = leak("t");
        let runs = Arc::new(AtomicU64::new(0));
        let counter = runs.clone();
        // Trois réveils espacés de 5 s, puis plus rien.
        let job = job(move || {
            let counter = counter.clone();
            async move { if counter.fetch_add(1, Ordering::SeqCst) < 2 { Some(5) } else { None } }
        });
        timer.restart(Some(5), job);
        advance(6).await;
        assert_eq!(runs.load(Ordering::SeqCst), 1);
        advance(5).await;
        assert_eq!(runs.load(Ordering::SeqCst), 2);
        advance(5).await;
        assert_eq!(runs.load(Ordering::SeqCst), 3);
        assert!(!timer.is_armed());
        advance(60).await;
        assert_eq!(runs.load(Ordering::SeqCst), 3);
    }

    #[tokio::test(start_paused = true)]
    async fn a_restart_never_interrupts_a_running_job() {
        let timer = leak("t");
        let finished = Arc::new(AtomicUsize::new(0));
        let done = finished.clone();
        // Le travail dure 20 s (par exemple une suite d'appels Discord).
        let slow = job(move || {
            let done = done.clone();
            async move {
                tokio::time::sleep(Duration::from_secs(20)).await;
                done.fetch_add(1, Ordering::SeqCst);
                None
            }
        });
        timer.restart(Some(1), slow);
        advance(5).await; // le travail a commencé
        let count = Arc::new(AtomicUsize::new(0));
        timer.restart(Some(100), counting_job(count.clone())); // reconnexion pendant le travail
        advance(20).await;
        assert_eq!(finished.load(Ordering::SeqCst), 1, "the running job completed");
    }

    #[tokio::test(start_paused = true)]
    async fn exclusive_waits_for_the_running_job() {
        let timer = leak("t");
        let order = Arc::new(StdMutex::new(Vec::new()));
        let log = order.clone();
        let slow = job(move || {
            let log = log.clone();
            async move {
                tokio::time::sleep(Duration::from_secs(20)).await;
                log.lock().unwrap().push("job");
                None
            }
        });
        timer.restart(Some(1), slow);
        advance(5).await;
        let guard = timer.exclusive().await; // attend la fin du travail (setup)
        order.lock().unwrap().push("setup");
        drop(guard);
        assert_eq!(*order.lock().unwrap(), vec!["job", "setup"]);
    }

    #[tokio::test(start_paused = true)]
    async fn a_job_waiting_behind_setup_is_skipped_when_setup_rearms() {
        let timer = leak("t");
        let count = Arc::new(AtomicUsize::new(0));
        timer.restart(Some(1), counting_job(count.clone()));
        let guard = timer.exclusive().await; // setup en cours : relit la base
        advance(5).await; // le réveil a sonné et attend le verrou
        timer.restart(Some(50), counting_job(count.clone())); // setup reprogramme
        drop(guard);
        advance(5).await;
        assert_eq!(count.load(Ordering::SeqCst), 0, "the stale wake-up did nothing");
        advance(50).await;
        assert_eq!(count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_stale_job_can_still_bring_the_next_wake_up_forward() {
        // Un traitement en cours réinsère un élément qui expire dans 5 s alors qu'un ajout
        // (génération plus récente) a programmé le réveil dans 100 s : on se réveille dans 5 s.
        let timer = leak("t");
        let calls = Arc::new(AtomicUsize::new(0));
        let counter = calls.clone();
        // 1er appel : traitement de 10 s qui réinsère un élément échéant 5 s plus tard.
        let work = job(move || {
            let counter = counter.clone();
            async move {
                if counter.fetch_add(1, Ordering::SeqCst) == 0 {
                    tokio::time::sleep(Duration::from_secs(10)).await;
                    Some(5)
                } else {
                    None
                }
            }
        });
        timer.restart(Some(1), work.clone());
        advance(2).await;
        timer.schedule(100, work);
        advance(10).await; // fin du traitement : réveil avancé à +5 s
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        advance(6).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2, "woken after 5 s, not 100 s");
    }

    #[tokio::test(start_paused = true)]
    async fn stop_cancels_the_wake_up() {
        let timer = leak("t");
        let count = Arc::new(AtomicUsize::new(0));
        timer.restart(Some(10), counting_job(count.clone()));
        timer.stop();
        assert!(!timer.is_armed());
        advance(20).await;
        assert_eq!(count.load(Ordering::SeqCst), 0);
    }
}
