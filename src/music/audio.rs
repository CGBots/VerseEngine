//! Contrôle d'un fichier audio envoyé par un admin (`MUSIC_SPEC.md` § 3.1).
//!
//! Deux barrières : la **signature** du fichier doit être celle d'un format accepté, puis le
//! fichier est **décodé d'un bout à l'autre** par Symphonia (Rust pur, sans code natif). Le type
//! servi aux joueurs vient de ce contrôle, jamais du nom du fichier ni du type annoncé.

use std::io::Cursor;

use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::registry::CodecRegistry;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;

/// Taille max d'un fichier (200 Mio : des heures d'ambiance). Les joueurs le lisent en continu
/// (`/api/music/stream/…`, par plages), sans le télécharger en entier.
pub const MAX_FILE_BYTES: usize = 200 * 1024 * 1024;
// Pas de durée maximale : une ambiance peut durer des heures. Seule la taille du fichier compte.
/// Part de paquets illisibles tolérée (certains encodeurs MP3 laissent un premier paquet abîmé).
const MAX_BAD_PACKETS_RATIO: f64 = 0.01;

/// Formats acceptés, reconnus à leur signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Container {
    Mp3,
    Ogg,
    Flac,
    Wav,
    Mp4,
    WebM,
}

impl Container {
    fn extension(self) -> &'static str {
        match self {
            Container::Mp3 => "mp3",
            Container::Ogg => "ogg",
            Container::Flac => "flac",
            Container::Wav => "wav",
            Container::Mp4 => "m4a",
            Container::WebM => "webm",
        }
    }

    /// Type MIME servi aux joueurs.
    pub fn mime(self) -> &'static str {
        match self {
            Container::Mp3 => "audio/mpeg",
            Container::Ogg => "audio/ogg",
            Container::Flac => "audio/flac",
            Container::Wav => "audio/wav",
            Container::Mp4 => "audio/mp4",
            Container::WebM => "audio/webm",
        }
    }
}

/// Résultat d'un contrôle réussi.
#[derive(Debug, Clone, PartialEq)]
pub struct AudioInfo {
    pub container: Container,
    pub duration_ms: u64,
    pub sample_rate: u32,
}

/// Raison d'un refus (clé de traduction côté activité).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioError {
    Empty,
    TooLarge,
    UnknownFormat,
    NotAudio,
    Undecodable,
}

impl AudioError {
    pub fn key(self) -> &'static str {
        match self {
            AudioError::Empty | AudioError::UnknownFormat => "activity__music_unknown_format",
            AudioError::TooLarge => "activity__payload_too_large",
            AudioError::NotAudio => "activity__music_not_audio",
            AudioError::Undecodable => "activity__music_undecodable",
        }
    }
}

/// Format d'après les premiers octets ; `None` s'il n'est pas accepté.
pub fn sniff(bytes: &[u8]) -> Option<Container> {
    let at = |i: usize, sig: &[u8]| bytes.len() >= i + sig.len() && &bytes[i..i + sig.len()] == sig;
    if at(0, b"OggS") {
        Some(Container::Ogg)
    } else if at(0, b"fLaC") {
        Some(Container::Flac)
    } else if at(0, b"RIFF") && at(8, b"WAVE") {
        Some(Container::Wav)
    } else if at(4, b"ftyp") {
        Some(Container::Mp4)
    } else if at(0, &[0x1A, 0x45, 0xDF, 0xA3]) {
        Some(Container::WebM)
    } else if at(0, b"ID3") || (bytes.len() >= 2 && bytes[0] == 0xFF && bytes[1] & 0xE0 == 0xE0) {
        Some(Container::Mp3)
    } else {
        None
    }
}

fn codecs() -> &'static CodecRegistry {
    static REGISTRY: std::sync::OnceLock<CodecRegistry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut registry = CodecRegistry::new();
        symphonia::default::register_enabled_codecs(&mut registry);
        registry.register_audio_decoder::<symphonia_opus::OpusDecoder>();
        registry
    })
}

/// Contrôle complet. Coûteux (décodage) : à appeler hors du fil de l'exécuteur asynchrone.
pub fn check_audio(bytes: &[u8]) -> Result<AudioInfo, AudioError> {
    if bytes.is_empty() {
        return Err(AudioError::Empty);
    }
    if bytes.len() > MAX_FILE_BYTES {
        return Err(AudioError::TooLarge);
    }
    let container = sniff(bytes).ok_or(AudioError::UnknownFormat)?;

    let source = MediaSourceStream::new(Box::new(Cursor::new(bytes.to_vec())), MediaSourceStreamOptions::default());
    let mut hint = Hint::new();
    hint.with_extension(container.extension());
    let mut reader = symphonia::default::get_probe()
        .probe(&hint, source, FormatOptions::default(), MetadataOptions::default())
        .map_err(|_| AudioError::Undecodable)?;

    // Un seul flux, audio : pas de vidéo hébergée sous couvert de musique.
    let tracks = reader.tracks();
    if tracks.iter().any(|t| matches!(t.codec_params, Some(CodecParameters::Video(_)) | Some(CodecParameters::Subtitle(_)))) {
        return Err(AudioError::NotAudio);
    }
    let track = reader.default_track(TrackType::Audio).ok_or(AudioError::NotAudio)?;
    let track_id = track.id;
    let params = match &track.codec_params {
        Some(CodecParameters::Audio(p)) => p.clone(),
        _ => return Err(AudioError::NotAudio),
    };
    let mut decoder = codecs().make_audio_decoder(&params, &AudioDecoderOptions::default()).map_err(|_| AudioError::Undecodable)?;

    let mut frames: u64 = 0;
    let mut sample_rate = params.sample_rate.unwrap_or(0);
    let (mut packets, mut bad) = (0u64, 0u64);
    loop {
        let packet = match reader.next_packet() {
            Ok(Some(p)) => p,
            Ok(None) => break,
            // Fin de flux signalée par une erreur d'E/S (fichier coupé net) : on garde ce qui a été lu.
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => return Err(AudioError::Undecodable),
        };
        if packet.track_id != track_id {
            continue;
        }
        packets += 1;
        match decoder.decode(&packet) {
            Ok(buf) => {
                frames += buf.frames() as u64;
                if sample_rate == 0 {
                    sample_rate = buf.spec().rate();
                }
            }
            Err(SymphoniaError::DecodeError(_)) => bad += 1,
            Err(_) => return Err(AudioError::Undecodable),
        }
    }
    if packets == 0 || frames == 0 || sample_rate == 0 || bad as f64 > (packets as f64 * MAX_BAD_PACKETS_RATIO).max(1.0) {
        return Err(AudioError::Undecodable);
    }
    Ok(AudioInfo { container, duration_ms: frames * 1000 / sample_rate as u64, sample_rate })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(format!("{}/tests/fixtures/audio/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
    }

    /// Durée de la vérification d'un gros fichier : `AUDIO_TIMING_FILE=<fichier> cargo test … -- --ignored --nocapture`.
    #[test]
    #[ignore = "measure: needs AUDIO_TIMING_FILE"]
    fn time_a_large_file_check() {
        let path = std::env::var("AUDIO_TIMING_FILE").expect("AUDIO_TIMING_FILE");
        let bytes = std::fs::read(path).unwrap();
        let start = std::time::Instant::now();
        let r = check_audio(&bytes);
        println!("{} Mo vérifiés en {:.1} s : {:?}", bytes.len() / 1_000_000, start.elapsed().as_secs_f64(), r.map(|i| i.duration_ms / 60_000));
    }

    #[test]
    fn every_accepted_format_is_decoded_and_timed() {
        for (name, container) in [
            ("tone.mp3", Container::Mp3),
            ("tone.ogg", Container::Ogg),
            ("tone.opus", Container::Ogg),
            ("tone.flac", Container::Flac),
            ("tone.wav", Container::Wav),
            ("tone.m4a", Container::Mp4),
            ("tone.webm", Container::WebM),
        ] {
            let info = check_audio(&fixture(name)).unwrap_or_else(|e| panic!("{name}: {e:?}"));
            assert_eq!(info.container, container, "{name}");
            assert!((1900..=2100).contains(&info.duration_ms), "{name}: {} ms", info.duration_ms);
        }
    }

    #[test]
    fn disguised_corrupt_or_video_files_are_refused() {
        assert_eq!(check_audio(&fixture("page.mp3")), Err(AudioError::UnknownFormat), "an HTML page named .mp3");
        assert_eq!(check_audio(&fixture("corrupt.ogg")), Err(AudioError::Undecodable));
        assert_eq!(check_audio(&fixture("header-only.flac")), Err(AudioError::Undecodable));
        assert_eq!(check_audio(&fixture("video.mp4")), Err(AudioError::NotAudio));
        assert_eq!(check_audio(b""), Err(AudioError::Empty));
        // Signature d'un format accepté, contenu quelconque.
        let mut fake = b"OggS".to_vec();
        fake.extend(std::iter::repeat_n(0x41, 5000));
        assert_eq!(check_audio(&fake), Err(AudioError::Undecodable));
    }

    #[test]
    fn a_cut_file_keeps_what_was_read() {
        let info = check_audio(&fixture("truncated.mp3")).unwrap();
        assert!(info.duration_ms < 1000, "{}", info.duration_ms);
    }

    #[test]
    fn size_is_checked_before_decoding() {
        assert_eq!(check_audio(&vec![0xFF; MAX_FILE_BYTES + 1]), Err(AudioError::TooLarge));
    }
}
