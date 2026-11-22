//! Récupération du son d'une vidéo YouTube avec yt-dlp (`MUSIC_SPEC.md` § 3.3, étape S5).

/// Import YouTube activé (`MUSIC_YTDLP_ENABLED=true`) ; coupé par défaut.
pub fn enabled() -> bool {
    std::env::var("MUSIC_YTDLP_ENABLED").is_ok_and(|v| v.eq_ignore_ascii_case("true"))
}
