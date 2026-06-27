//! Spike/QA IA — Fase 4 (Whisper): transcribe el audio de un archivo usando el
//! ENGINE REAL de la app (`diskdex_lib::ai::whisper_engine`), no una copia. Así
//! esta herramienta verifica exactamente el camino que corre en producción
//! (incluido el fallback de temperatura).
//!
//! Gateado por la feature `ai`. Uso:
//!   cargo run --profile release-ai --features ai,accel --bin whisper_probe -- <archivo.mp4>
//!   DISKDEX_WHISPER_MODEL=openai/whisper-small cargo run ... -- <archivo>

#[cfg(feature = "accel")]
extern crate accelerate_src;

use anyhow::{Context, Result};
use diskdex_lib::{ai, video};
use std::path::PathBuf;

fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("uso: whisper_probe <archivo de audio/video>")?;
    eprintln!("extrayendo audio de {path}…");
    let pcm = video::extract_audio_pcm(&PathBuf::from(&path)).map_err(anyhow::Error::msg)?;
    eprintln!("PCM: {} muestras (~{:.1}s)", pcm.len(), pcm.len() as f32 / 16000.0);

    eprintln!("cargando engine Whisper (la 1ª vez descarga el modelo)…");
    let engine = ai::whisper_engine().context("modelo Whisper")?;
    let t0 = std::time::Instant::now();
    let (lang, text) = {
        let mut e = engine.lock().unwrap();
        e.transcribe(&pcm)?
    };
    eprintln!("idioma detectado: {lang}  ·  transcripción en {:?}\n", t0.elapsed());
    println!("{text}");
    Ok(())
}
