//! Vidéos YouTube de la bibliothèque musicale (`MUSIC_SPEC.md` § 3.3).
//!
//! Rien n'est téléchargé : le bot garde l'identifiant de la vidéo, et l'activité la lit avec le
//! **lecteur officiel de YouTube**, visible (200 × 200 au moins), comme le demandent les règles
//! des développeurs YouTube. Le son va directement de YouTube au navigateur du joueur.

pub fn valid_id(id: &str) -> bool {
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
}
