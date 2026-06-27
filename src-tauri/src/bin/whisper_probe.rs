//! Spike IA — Fase 4 (Whisper): de-riesgo de la transcripción de audio.
//!
//! Extrae el audio de un archivo con ffmpeg (→ PCM f32 mono 16 kHz), calcula el
//! mel-spectrogram, corre Whisper base (multilingüe) y transcribe — detectando el
//! idioma (debería dar "spanish" en tu material). Greedy (sin temperature
//! fallback ni rand) para minimizar deps; suficiente para probar que anda.
//!
//! Gateado por la feature `ai` (apagada por default). Uso:
//!   cargo run --profile release-ai --features ai,accel --bin whisper_probe -- <archivo.mp4>
//!   WHISPER_MODEL=openai/whisper-small cargo run ... --bin whisper_probe -- <archivo>
//!
//! La 1ª vez descarga el modelo (~145 MB base / ~480 MB small) a ~/.cache/huggingface.

#[cfg(feature = "accel")]
extern crate accelerate_src;

use anyhow::{bail, Context, Result};
use candle_core::{Device, IndexOp, Tensor};
use candle_nn::{ops::softmax, VarBuilder};
use candle_transformers::models::whisper::{self as m, audio, model::Whisper, Config};
use std::process::Command;
use tokenizers::Tokenizer;

fn token_id(tokenizer: &Tokenizer, token: &str) -> Result<u32> {
    tokenizer
        .token_to_id(token)
        .with_context(|| format!("sin token-id para {token}"))
}

/// Extrae el audio a PCM f32 mono 16 kHz vía ffmpeg.
fn extract_pcm(path: &str) -> Result<Vec<f32>> {
    let out = Command::new("ffmpeg")
        .args(["-v", "quiet", "-i", path, "-ar", "16000", "-ac", "1", "-f", "f32le", "-"])
        .output()
        .context("no se pudo ejecutar ffmpeg")?;
    if !out.status.success() || out.stdout.is_empty() {
        bail!("ffmpeg no pudo extraer audio (¿el archivo tiene pista de audio?)");
    }
    Ok(out
        .stdout
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect())
}

/// Detecta el idioma → token `<|xx|>` (puerto del módulo multilingual de candle).
fn detect_language(model: &mut Whisper, tokenizer: &Tokenizer, mel: &Tensor) -> Result<u32> {
    const LANGS: &[&str] = &[
        "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv",
        "it", "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no",
        "th", "ur", "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr",
        "az", "sl", "kn", "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw",
        "gl", "mr", "pa", "si", "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu",
        "am", "yi", "lo", "uz", "fo", "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl",
        "mg", "as", "tt", "haw", "ln", "ha", "ba", "jw", "su",
    ];
    let (_b, _, seq_len) = mel.dims3()?;
    let mel = mel.narrow(2, 0, usize::min(seq_len, model.config.max_source_positions))?;
    let device = mel.device();
    let lang_ids = LANGS
        .iter()
        .map(|t| token_id(tokenizer, &format!("<|{t}|>")))
        .collect::<Result<Vec<_>>>()?;
    let sot = token_id(tokenizer, m::SOT_TOKEN)?;
    let audio_features = model.encoder.forward(&mel, true)?;
    let tokens = Tensor::new(&[[sot]], device)?;
    let lang_ids_t = Tensor::new(lang_ids.as_slice(), device)?;
    let ys = model.decoder.forward(&tokens, &audio_features, true)?;
    let logits = model.decoder.final_linear(&ys.i(..1)?)?.i(0)?.i(0)?;
    let logits = logits.index_select(&lang_ids_t, 0)?;
    let probs = softmax(&logits, candle_core::D::Minus1)?.to_vec1::<f32>()?;
    let mut ranked: Vec<(&str, f32)> = LANGS.iter().copied().zip(probs).collect();
    ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
    for (l, p) in ranked.iter().take(3) {
        eprintln!("  idioma {l}: {p:.3}");
    }
    token_id(tokenizer, &format!("<|{}|>", ranked[0].0))
}

fn main() -> Result<()> {
    let path = std::env::args()
        .nth(1)
        .context("uso: whisper_probe <archivo de audio/video>")?;
    let repo = std::env::var("WHISPER_MODEL").unwrap_or_else(|_| "openai/whisper-base".to_string());
    let device = Device::Cpu;

    eprintln!("extrayendo audio de {path}…");
    let pcm = extract_pcm(&path)?;
    eprintln!("PCM: {} muestras (~{:.1}s)", pcm.len(), pcm.len() as f32 / 16000.0);

    eprintln!("cargando whisper {repo} (la 1ª vez descarga)…");
    let api = hf_hub::api::sync::Api::new()?.model(repo.clone());
    let config: Config = serde_json::from_slice(&std::fs::read(api.get("config.json")?)?)?;
    let tokenizer = Tokenizer::from_file(api.get("tokenizer.json")?).map_err(anyhow::Error::msg)?;
    let weights = api.get("model.safetensors")?;

    // Mel filterbank bundleado (80 bins).
    if config.num_mel_bins != 80 {
        bail!("este probe asume 80 mel bins (whisper base/small); el modelo tiene {}", config.num_mel_bins);
    }
    let mel_bytes = include_bytes!("../melfilters.bytes");
    let mel_filters: Vec<f32> = mel_bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();

    let mel = audio::pcm_to_mel(&config, &pcm, &mel_filters);
    let n_mel = config.num_mel_bins;
    let mel = Tensor::from_vec(mel.clone(), (1, n_mel, mel.len() / n_mel), &device)?;

    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights], m::DTYPE, &device)? };
    let mut model = Whisper::load(&vb, config)?;

    let lang_token = detect_language(&mut model, &tokenizer, &mel)?;
    let sot = token_id(&tokenizer, m::SOT_TOKEN)?;
    let transcribe = token_id(&tokenizer, m::TRANSCRIBE_TOKEN)?;
    let eot = token_id(&tokenizer, m::EOT_TOKEN)?;
    let no_ts = token_id(&tokenizer, m::NO_TIMESTAMPS_TOKEN)?;

    // Tokens a suprimir (del config).
    let suppress: Vec<f32> = (0..model.config.vocab_size as u32)
        .map(|i| {
            if model.config.suppress_tokens.contains(&i) {
                f32::NEG_INFINITY
            } else {
                0.0
            }
        })
        .collect();
    let suppress = Tensor::new(suppress.as_slice(), &device)?;

    // Recorre ventanas de 30s y decodifica greedy.
    let (_, _, content_frames) = mel.dims3()?;
    let mut seek = 0usize;
    let mut full = String::new();
    let t0 = std::time::Instant::now();
    while seek < content_frames {
        let seg = usize::min(content_frames - seek, m::N_FRAMES);
        let mel_seg = mel.narrow(2, seek, seg)?;
        seek += seg;

        let audio_features = model.encoder.forward(&mel_seg, true)?;
        let mut tokens = vec![sot, lang_token, transcribe, no_ts];
        let sample_len = model.config.max_target_positions / 2;
        for i in 0..sample_len {
            let tokens_t = Tensor::new(tokens.as_slice(), &device)?.unsqueeze(0)?;
            let ys = model.decoder.forward(&tokens_t, &audio_features, i == 0)?;
            let (_, sl, _) = ys.dims3()?;
            let logits = model
                .decoder
                .final_linear(&ys.i((..1, sl - 1..))?)?
                .i(0)?
                .i(0)?;
            let logits = logits.broadcast_add(&suppress)?;
            let next = logits
                .to_vec1::<f32>()?
                .into_iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.total_cmp(b))
                .map(|(i, _)| i as u32)
                .unwrap();
            tokens.push(next);
            if next == eot || tokens.len() > model.config.max_target_positions {
                break;
            }
        }
        let text = tokenizer.decode(&tokens, true).map_err(anyhow::Error::msg)?;
        full.push_str(text.trim());
        full.push(' ');
    }
    eprintln!("transcripción en {:?}\n", t0.elapsed());
    println!("{}", full.trim());
    Ok(())
}
