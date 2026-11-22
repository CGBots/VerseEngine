//! Récupération du son d'une vidéo YouTube avec yt-dlp (`MUSIC_SPEC.md` § 3.3, étape S5).
//!
//! Choix de l'utilisateur (décision M1), encadré :
//! - coupé par défaut (`MUSIC_YTDLP_ENABLED=true` pour l'activer) ;
//! - seul un identifiant de vidéo est accepté ; l'URL donnée à yt-dlp est reconstruite ici ;
//! - yt-dlp est lancé **sans shell**, avec des arguments fixes, sans configuration ni extensions,
//!   un environnement vidé, un dossier temporaire à lui, un délai et une taille maximum ;
//! - le fichier obtenu passe ensuite par les mêmes contrôles qu'un fichier envoyé
//!   (`music::audio::check_audio`).

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use crate::music::audio::{MAX_DURATION_MS, MAX_FILE_BYTES};

/// Délai maximum d'une extraction.
pub const TIMEOUT: Duration = Duration::from_secs(180);
/// Extractions simultanées sur tout le bot.
const MAX_CONCURRENT: usize = 2;

/// Import YouTube activé (`MUSIC_YTDLP_ENABLED=true`) ; coupé par défaut.
pub fn enabled() -> bool {
    std::env::var("MUSIC_YTDLP_ENABLED").is_ok_and(|v| v.eq_ignore_ascii_case("true"))
}

fn valid_id(id: &str) -> bool {
    id.len() == 11 && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// Identifiant de la vidéo d'une URL YouTube (youtube.com, m., music., youtu.be ; `watch`,
/// `shorts`, `embed`) ou d'un identifiant seul. `None` pour tout le reste (listes de lecture
/// seules, autres sites, URL trafiquées).
pub fn video_id(input: &str) -> Option<String> {
    let input = input.trim();
    if valid_id(input) {
        return Some(input.to_string());
    }
    let rest = input.strip_prefix("https://").or_else(|| input.strip_prefix("http://"))?;
    let (host, path_query) = rest.split_once('/').unwrap_or((rest, ""));
    let host = host.to_ascii_lowercase();
    let (path, query) = path_query.split_once('?').unwrap_or((path_query, ""));
    let path = path.split('#').next().unwrap_or("");
    let candidate = match host.as_str() {
        "youtu.be" => path.split('/').next().map(str::to_string),
        "youtube.com" | "www.youtube.com" | "m.youtube.com" | "music.youtube.com" => {
            let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
            match segments.as_slice() {
                ["watch"] => query.split('#').next().unwrap_or("").split('&').find_map(|kv| kv.strip_prefix("v=")).map(str::to_string),
                ["shorts" | "embed" | "v", id, ..] => Some(id.to_string()),
                _ => None,
            }
        }
        _ => None,
    }?;
    valid_id(&candidate).then_some(candidate)
}

/// Arguments de yt-dlp (fixes : seuls l'identifiant et les chemins varient).
pub fn command_args(id: &str, out_dir: &Path, deno: Option<&str>) -> Vec<String> {
    assert!(valid_id(id), "video id checked by the caller");
    let mut args: Vec<String> = [
        "--ignore-config",
        "--no-plugin-dirs",
        "--no-remote-components",
        "--no-playlist",
        "--no-exec",
        "--no-cache-dir",
        "--no-part",
        "--no-mtime",
        "--no-progress",
        "--quiet",
        "--no-warnings",
        "--abort-on-error",
        "--socket-timeout",
        "20",
    ]
    .into_iter()
    .map(String::from)
    .collect();
    if let Some(deno) = deno {
        args.extend(["--no-js-runtimes".into(), "--js-runtimes".into(), format!("deno:{deno}")]);
    }
    args.extend([
        "--max-filesize".into(),
        MAX_FILE_BYTES.to_string(),
        "--match-filters".into(),
        format!("!is_live & duration <= {}", MAX_DURATION_MS / 1000),
        // Meilleur flux audio seul, dans un format que le contrôle sait lire (pas de ffmpeg).
        "-f".into(),
        "bestaudio[ext=m4a]/bestaudio[ext=webm]/bestaudio[acodec=opus]/bestaudio".into(),
        "--no-simulate".into(),
        "--print".into(),
        "after_move:title".into(),
        "-o".into(),
        format!("{}/audio.%(ext)s", out_dir.display()),
        "--".into(),
        format!("https://www.youtube.com/watch?v={id}"),
    ]);
    args
}

/// Raison d'un échec (clé de traduction côté activité).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum YtError {
    /// yt-dlp a échoué ou n'a rien produit (vidéo privée, en direct, trop longue, bloquée…).
    Unavailable,
    TooLarge,
    Timeout,
    /// yt-dlp introuvable ou impossible à lancer.
    NotInstalled,
}

impl YtError {
    pub fn key(self) -> &'static str {
        match self {
            YtError::Unavailable => "activity__music_youtube_unavailable",
            YtError::TooLarge => "activity__payload_too_large",
            YtError::Timeout => "activity__music_youtube_timeout",
            YtError::NotInstalled => "activity__music_youtube_disabled",
        }
    }
}

pub struct Fetched {
    pub bytes: Vec<u8>,
    pub title: Option<String>,
}

/// Dossier temporaire supprimé quoi qu'il arrive.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let dir = std::env::temp_dir().join(format!("verseengine-ytdlp-{}", mongodb::bson::oid::ObjectId::new().to_hex()));
        std::fs::create_dir(&dir)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        }
        Ok(Self(dir))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn semaphore() -> &'static tokio::sync::Semaphore {
    static SEM: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    SEM.get_or_init(|| tokio::sync::Semaphore::new(MAX_CONCURRENT))
}

/// Binaire à lancer : `MUSIC_YTDLP_PATH` (défaut : `yt-dlp` dans le PATH).
fn binary() -> String {
    std::env::var("MUSIC_YTDLP_PATH").unwrap_or_else(|_| "yt-dlp".into())
}

/// Télécharge le meilleur flux audio de la vidéo `id` (identifiant déjà vérifié).
pub async fn fetch(id: &str) -> Result<Fetched, YtError> {
    fetch_with(&binary(), std::env::var("MUSIC_YTDLP_DENO").ok().as_deref(), id, TIMEOUT).await
}

pub async fn fetch_with(binary: &str, deno: Option<&str>, id: &str, timeout: Duration) -> Result<Fetched, YtError> {
    if !valid_id(id) {
        return Err(YtError::Unavailable);
    }
    let _permit = semaphore().acquire().await.map_err(|_| YtError::Unavailable)?;
    let dir = TempDir::new().map_err(|_| YtError::Unavailable)?;
    // Environnement vidé : rien du bot (jetons, base de données) n'est transmis à yt-dlp.
    let mut path = String::from("/usr/local/bin:/usr/bin:/bin");
    if let Some(parent) = Path::new(binary).parent().filter(|p| !p.as_os_str().is_empty()) {
        path = format!("{}:{path}", parent.display());
    }
    let child = tokio::process::Command::new(binary)
        .args(command_args(id, &dir.0, deno))
        .env_clear()
        .env("PATH", path)
        .env("HOME", &dir.0)
        .env("LANG", "C.UTF-8")
        .current_dir(&dir.0)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| {
            tracing::warn!(error = %e, "music: yt-dlp introuvable");
            YtError::NotInstalled
        })?;
    let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
        Ok(Ok(o)) => o,
        Ok(Err(_)) => return Err(YtError::Unavailable),
        // Le processus est tué (kill_on_drop) avec `child`.
        Err(_) => return Err(YtError::Timeout),
    };
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!(video = id, stderr = %stderr.chars().take(2000).collect::<String>(), "music: yt-dlp a échoué");
        return Err(YtError::Unavailable);
    }
    let file = std::fs::read_dir(&dir.0)
        .map_err(|_| YtError::Unavailable)?
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().starts_with("audio.") && e.file_type().is_ok_and(|t| t.is_file()))
        .ok_or(YtError::Unavailable)?;
    let len = file.metadata().map_err(|_| YtError::Unavailable)?.len();
    if len as usize > MAX_FILE_BYTES {
        return Err(YtError::TooLarge);
    }
    let bytes = tokio::fs::read(file.path()).await.map_err(|_| YtError::Unavailable)?;
    let title = String::from_utf8_lossy(&output.stdout).lines().map(str::trim).find(|l| !l.is_empty()).map(str::to_string);
    Ok(Fetched { bytes, title })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_ids_are_taken_only_from_youtube_urls() {
        let id = "dQw4w9WgXcQ";
        for url in [
            "dQw4w9WgXcQ",
            "https://www.youtube.com/watch?v=dQw4w9WgXcQ",
            "https://youtube.com/watch?feature=share&v=dQw4w9WgXcQ&t=42",
            "https://m.youtube.com/watch?v=dQw4w9WgXcQ#t=1",
            "https://music.youtube.com/watch?v=dQw4w9WgXcQ&list=RD",
            "https://youtu.be/dQw4w9WgXcQ?si=abc",
            "https://www.youtube.com/shorts/dQw4w9WgXcQ",
            "https://www.youtube.com/embed/dQw4w9WgXcQ",
            "  https://WWW.YOUTUBE.COM/watch?v=dQw4w9WgXcQ  ",
        ] {
            assert_eq!(video_id(url).as_deref(), Some(id), "{url}");
        }
        for bad in [
            "",
            "https://www.youtube.com/playlist?list=PL123",
            "https://www.youtube.com/watch?v=short",
            "https://www.youtube.com.evil.example/watch?v=dQw4w9WgXcQ",
            "https://evil.example/?u=https://youtu.be/dQw4w9WgXcQ",
            "https://youtu.be/dQw4w9WgXcQ;rm -rf",
            "file:///etc/passwd",
            "--exec=sh",
            "https://www.youtube.com/watch?v=dQw4w9WgXc$",
        ] {
            assert_eq!(video_id(bad), None, "{bad}");
        }
    }

    #[test]
    fn the_command_is_fixed_and_safe() {
        let args = command_args("dQw4w9WgXcQ", Path::new("/tmp/x"), None);
        for needed in ["--ignore-config", "--no-plugin-dirs", "--no-exec", "--no-playlist", "--no-remote-components"] {
            assert!(args.iter().any(|a| a == needed), "{needed}");
        }
        assert_eq!(args.last().unwrap(), "https://www.youtube.com/watch?v=dQw4w9WgXcQ");
        assert_eq!(args[args.len() - 2], "--", "the URL can never be read as an option");
        assert!(args.contains(&MAX_FILE_BYTES.to_string()));
        assert!(args.contains(&"!is_live & duration <= 900".to_string()));
        let with_deno = command_args("dQw4w9WgXcQ", Path::new("/tmp/x"), Some("/opt/yt-dlp/bin/deno"));
        assert!(with_deno.contains(&"deno:/opt/yt-dlp/bin/deno".to_string()));
    }

    /// Faux yt-dlp : vérifie quelques arguments, copie `source` à l'emplacement demandé et
    /// affiche un titre ; ou échoue, ou ne répond pas.
    #[cfg(unix)]
    fn fake(behaviour: &str, source: &str) -> (TempDir, String) {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new().unwrap();
        let script = dir.0.join("yt-dlp");
        let body = match behaviour {
            "ok" => format!(
                r#"#!/bin/sh
case " $* " in *" --ignore-config "*) ;; *) echo "missing --ignore-config" >&2; exit 2 ;; esac
[ -z "$DISCORD_TOKEN$MONGODB_PASSWORD" ] || {{ echo "secret leaked" >&2; exit 3; }}
out=""
while [ $# -gt 0 ]; do [ "$1" = "-o" ] && out="$2"; shift; done
cp "{source}" "$(echo "$out" | sed 's/%(ext)s/webm/')"
echo "Chanson de la taverne"
"#
            ),
            "fail" => "#!/bin/sh\necho 'ERROR: Video unavailable' >&2\nexit 1\n".into(),
            "nothing" => "#!/bin/sh\nexit 0\n".into(),
            _ => "#!/bin/sh\nsleep 30\n".into(),
        };
        std::fs::write(&script, body).unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let path = script.display().to_string();
        (dir, path)
    }

    #[cfg(unix)]
    #[test]
    fn a_fake_yt_dlp_is_run_safely() {
        let audio = format!("{}/tests/fixtures/audio/tone.webm", env!("CARGO_MANIFEST_DIR"));
        crate::test_support::block_on(async {
            // Les tests tournent avec DISCORD_TOKEN défini : le faux yt-dlp échoue s'il le voit.
            let (_d, ok) = fake("ok", &audio);
            let got = fetch_with(&ok, None, "dQw4w9WgXcQ", Duration::from_secs(10)).await.unwrap_or_else(|e| panic!("{e:?}"));
            assert_eq!(got.bytes, std::fs::read(&audio).unwrap());
            assert_eq!(got.title.as_deref(), Some("Chanson de la taverne"));
            assert_eq!(crate::music::audio::check_audio(&got.bytes).unwrap().container, crate::music::audio::Container::WebM);

            let (_d, fail) = fake("fail", &audio);
            assert_eq!(fetch_with(&fail, None, "dQw4w9WgXcQ", Duration::from_secs(10)).await.err(), Some(YtError::Unavailable));
            let (_d, nothing) = fake("nothing", &audio);
            assert_eq!(fetch_with(&nothing, None, "dQw4w9WgXcQ", Duration::from_secs(10)).await.err(), Some(YtError::Unavailable));
            let (_d, slow) = fake("slow", &audio);
            assert_eq!(fetch_with(&slow, None, "dQw4w9WgXcQ", Duration::from_millis(300)).await.err(), Some(YtError::Timeout));
            assert_eq!(fetch_with("/nonexistent/yt-dlp", None, "dQw4w9WgXcQ", Duration::from_secs(1)).await.err(), Some(YtError::NotInstalled));
            assert_eq!(fetch_with(&ok, None, "bad id", Duration::from_secs(1)).await.err(), Some(YtError::Unavailable));
            // Aucun dossier temporaire ne reste.
            let left = std::fs::read_dir(std::env::temp_dir()).unwrap().filter_map(Result::ok).filter(|e| {
                let n = e.file_name().to_string_lossy().to_string();
                n.starts_with("verseengine-ytdlp-") && e.path().join("audio.webm").exists()
            }).count();
            assert_eq!(left, 0);
        });
    }
}
