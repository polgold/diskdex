//! IA local (Fase 1): búsqueda semántica de imágenes con SigLIP 2 multilingüe
//! vía candle. Todo en CPU (Accelerate con la feature `accel`), sin servicios
//! externos. El modelo se descarga/cachea en ~/.cache/huggingface la 1ª vez.
//!
//! Gateado por la feature `ai` (apagada por default → no infla el build normal).
//! Validado en el spike `embed_probe`: ver memoria del proyecto.

use anyhow::Result;
use candle_core::{DType, Device, IndexOp, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::siglip;
use std::sync::{Mutex, OnceLock};
use tokenizers::Tokenizer;

/// Modelo elegido en el spike: SigLIP 2 base patch16-256 (multilingüe vía Gemma).
pub const MODEL_REPO: &str = "google/siglip2-base-patch16-256";

/// Plantilla zero-shot que mejora el ranking vs palabra suelta (validado en spike).
const TEMPLATE: &str = "una foto de {}";

/// Engine cargado una vez y compartido (la inferencia se serializa con el Mutex).
pub struct Engine {
    model: siglip::Model,
    tokenizer: Tokenizer,
    device: Device,
    image_size: usize,
    max_len: usize,
    pad_id: u32,
}

static ENGINE: OnceLock<Mutex<Engine>> = OnceLock::new();

fn build_engine() -> Result<Engine> {
    let device = Device::Cpu;
    let api = hf_hub::api::sync::Api::new()?.model(MODEL_REPO.to_string());
    let model_file = api.get("model.safetensors")?;
    let config_file = api.get("config.json")?;
    let tok_file = api.get("tokenizer.json")?;
    let config: siglip::Config = serde_json::from_slice(&std::fs::read(config_file)?)?;
    let tokenizer = Tokenizer::from_file(tok_file).map_err(anyhow::Error::msg)?;
    let image_size = config.vision_config.image_size;
    let max_len = config.text_config.max_position_embeddings;
    let pad_id = tokenizer
        .token_to_id("<pad>")
        .unwrap_or(config.text_config.pad_token_id);
    let vb =
        unsafe { VarBuilder::from_mmaped_safetensors(&[model_file], DType::F32, &device)? };
    let model = siglip::Model::new(&config, vb)?;
    Ok(Engine {
        model,
        tokenizer,
        device,
        image_size,
        max_len,
        pad_id,
    })
}

/// Devuelve el engine, inicializándolo la 1ª vez (descarga/carga el modelo; puede
/// tardar varios segundos). Las llamadas siguientes son instantáneas.
pub fn engine() -> Result<&'static Mutex<Engine>> {
    if let Some(e) = ENGINE.get() {
        return Ok(e);
    }
    let built = build_engine()?;
    // Si otra llamada ganó la carrera, descartamos la nuestra y usamos la suya.
    let _ = ENGINE.set(Mutex::new(built));
    Ok(ENGINE.get().expect("engine recién seteado"))
}

/// ¿El modelo ya está en memoria? (para status sin forzar la carga/descarga).
pub fn is_loaded() -> bool {
    ENGINE.get().is_some()
}

fn l2_normalize(t: &Tensor) -> candle_core::Result<Tensor> {
    let norm = t.sqr()?.sum_keepdim(D::Minus1)?.sqrt()?;
    t.broadcast_div(&norm)
}

impl Engine {
    /// JPEG/PNG en bytes → tensor [3, S, S] f32 en [-1, 1] (preproc SigLIP).
    fn image_tensor(&self, bytes: &[u8]) -> Result<Tensor> {
        let img = image::load_from_memory(bytes)?
            .resize_to_fill(
                self.image_size as u32,
                self.image_size as u32,
                image::imageops::FilterType::Triangle,
            )
            .to_rgb8();
        let data = img.into_raw();
        let t = Tensor::from_vec(data, (self.image_size, self.image_size, 3), &self.device)?
            .permute((2, 0, 1))?
            .to_dtype(DType::F32)?
            .affine(2.0 / 255.0, -1.0)?;
        Ok(t)
    }

    /// Embebe un lote de imágenes (bytes) → vectores L2-normalizados.
    pub fn embed_images(&self, items: &[Vec<u8>]) -> Result<Vec<Vec<f32>>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let mut ts = Vec::with_capacity(items.len());
        for b in items {
            ts.push(self.image_tensor(b)?);
        }
        let imgs = Tensor::stack(&ts, 0)?;
        let feats = l2_normalize(&self.model.get_image_features(&imgs)?)?;
        Ok(feats.to_vec2::<f32>()?)
    }

    /// Embebe una query de texto (aplica plantilla + normaliza) → vector.
    pub fn embed_text(&self, query: &str) -> Result<Vec<f32>> {
        let prompt = TEMPLATE.replace("{}", &query.to_lowercase());
        let enc = self
            .tokenizer
            .encode(prompt, true)
            .map_err(anyhow::Error::msg)?;
        let mut ids = enc.get_ids().to_vec();
        ids.truncate(self.max_len);
        while ids.len() < self.max_len {
            ids.push(self.pad_id);
        }
        let input_ids = Tensor::new(vec![ids], &self.device)?;
        let feats = l2_normalize(&self.model.get_text_features(&input_ids)?)?;
        let mut v = feats.to_vec2::<f32>()?;
        Ok(v.remove(0))
    }
}

// ============================================================================
// Whisper (IA Fase 4) — transcripción de audio local. Modelo aparte del SigLIP.
// ============================================================================

use candle_transformers::models::whisper::{self as whisper, model::Whisper};

/// Modelo Whisper por defecto (multilingüe, 80 mel bins). Override en runtime con
/// la env `DISKDEX_WHISPER_MODEL` (p.ej. `openai/whisper-small` para más calidad).
pub const WHISPER_REPO: &str = "openai/whisper-base";

/// Códigos de idioma que Whisper puede detectar (los 99 del modelo).
const WHISPER_LANGS: &[&str] = &[
    "en", "zh", "de", "es", "ru", "ko", "fr", "ja", "pt", "tr", "pl", "ca", "nl", "ar", "sv", "it",
    "id", "hi", "fi", "vi", "he", "uk", "el", "ms", "cs", "ro", "da", "hu", "ta", "no", "th", "ur",
    "hr", "bg", "lt", "la", "mi", "ml", "cy", "sk", "te", "fa", "lv", "bn", "sr", "az", "sl", "kn",
    "et", "mk", "br", "eu", "is", "hy", "ne", "mn", "bs", "kk", "sq", "sw", "gl", "mr", "pa", "si",
    "km", "sn", "yo", "so", "af", "oc", "ka", "be", "tg", "sd", "gu", "am", "yi", "lo", "uz", "fo",
    "ht", "ps", "tk", "nn", "mt", "sa", "lb", "my", "bo", "tl", "mg", "as", "tt", "haw", "ln", "ha",
    "ba", "jw", "su",
];

pub struct WhisperEngine {
    model: Whisper,
    tokenizer: Tokenizer,
    device: Device,
    mel_filters: Vec<f32>,
    suppress: Tensor,
    sot: u32,
    transcribe: u32,
    eot: u32,
    no_ts: u32,
    no_speech: u32,
    lang_tokens: Vec<(&'static str, u32)>,
    rng: rand::rngs::StdRng,
}

static WHISPER_ENGINE: OnceLock<Mutex<WhisperEngine>> = OnceLock::new();

fn wtoken(tokenizer: &Tokenizer, t: &str) -> Result<u32> {
    tokenizer
        .token_to_id(t)
        .ok_or_else(|| anyhow::anyhow!("sin token-id para {t}"))
}

fn build_whisper() -> Result<WhisperEngine> {
    let repo = std::env::var("DISKDEX_WHISPER_MODEL").unwrap_or_else(|_| WHISPER_REPO.to_string());
    let device = Device::Cpu;
    let api = hf_hub::api::sync::Api::new()?.model(repo);
    let config: whisper::Config = serde_json::from_slice(&std::fs::read(api.get("config.json")?)?)?;
    let tokenizer = Tokenizer::from_file(api.get("tokenizer.json")?).map_err(anyhow::Error::msg)?;
    let weights = api.get("model.safetensors")?;
    if config.num_mel_bins != 80 {
        anyhow::bail!("este integrador asume 80 mel bins (whisper base/small)");
    }
    let mel_bytes = include_bytes!("melfilters.bytes");
    let mel_filters: Vec<f32> = mel_bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[weights], whisper::DTYPE, &device)? };
    let model = Whisper::load(&vb, config)?;
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
    let lang_tokens = WHISPER_LANGS
        .iter()
        .filter_map(|&c| wtoken(&tokenizer, &format!("<|{c}|>")).ok().map(|id| (c, id)))
        .collect();
    let no_speech = whisper::NO_SPEECH_TOKENS
        .iter()
        .find_map(|t| wtoken(&tokenizer, t).ok())
        .ok_or_else(|| anyhow::anyhow!("sin token de no-speech"))?;
    use rand::SeedableRng;
    Ok(WhisperEngine {
        sot: wtoken(&tokenizer, whisper::SOT_TOKEN)?,
        transcribe: wtoken(&tokenizer, whisper::TRANSCRIBE_TOKEN)?,
        eot: wtoken(&tokenizer, whisper::EOT_TOKEN)?,
        no_ts: wtoken(&tokenizer, whisper::NO_TIMESTAMPS_TOKEN)?,
        no_speech,
        lang_tokens,
        // Seed fija → transcripciones reproducibles entre corridas.
        rng: rand::rngs::StdRng::seed_from_u64(42),
        suppress,
        mel_filters,
        tokenizer,
        device,
        model,
    })
}

/// Engine Whisper (lazy: descarga/carga el modelo la 1ª vez).
pub fn whisper_engine() -> Result<&'static Mutex<WhisperEngine>> {
    if let Some(e) = WHISPER_ENGINE.get() {
        return Ok(e);
    }
    let built = build_whisper()?;
    let _ = WHISPER_ENGINE.set(Mutex::new(built));
    Ok(WHISPER_ENGINE.get().expect("whisper recién seteado"))
}

pub fn whisper_loaded() -> bool {
    WHISPER_ENGINE.get().is_some()
}

impl WhisperEngine {
    fn detect_language(&mut self, mel: &Tensor) -> Result<(&'static str, u32)> {
        let (_b, _, seq) = mel.dims3()?;
        let n = usize::min(seq, self.model.config.max_source_positions);
        let mel = mel.narrow(2, 0, n)?;
        let af = self.model.encoder.forward(&mel, true)?;
        let tokens = Tensor::new(&[[self.sot]], &self.device)?;
        let ys = self.model.decoder.forward(&tokens, &af, true)?;
        let logits = self.model.decoder.final_linear(&ys.i(..1)?)?.i(0)?.i(0)?;
        let ids: Vec<u32> = self.lang_tokens.iter().map(|(_, id)| *id).collect();
        let ids_t = Tensor::new(ids.as_slice(), &self.device)?;
        let logits = logits.index_select(&ids_t, 0)?;
        let probs = candle_nn::ops::softmax(&logits, D::Minus1)?.to_vec1::<f32>()?;
        let best = probs
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i)
            .unwrap_or(0);
        Ok(self.lang_tokens[best])
    }

    /// Decodifica una ventana a temperatura `t` (0 = greedy, >0 = sampling).
    /// Devuelve (texto, avg_logprob, no_speech_prob) para decidir reintentos.
    fn decode(
        &mut self,
        audio_features: &Tensor,
        lang_token: u32,
        t: f64,
    ) -> Result<(String, f64, f64)> {
        use rand::distr::{weighted::WeightedIndex, Distribution};
        let mut tokens = vec![self.sot, lang_token, self.transcribe, self.no_ts];
        let sample_len = self.model.config.max_target_positions / 2;
        let mut sum_logprob = 0f64;
        let mut no_speech_prob = 0f64;
        for i in 0..sample_len {
            let tokens_t = Tensor::new(tokens.as_slice(), &self.device)?.unsqueeze(0)?;
            let ys = self.model.decoder.forward(&tokens_t, audio_features, i == 0)?;
            // Prob de "sin voz" en el 1er paso (mirando el token no_speech).
            if i == 0 {
                let logits0 = self.model.decoder.final_linear(&ys.i(..1)?)?.i(0)?.i(0)?;
                no_speech_prob = candle_nn::ops::softmax(&logits0, 0)?
                    .i(self.no_speech as usize)?
                    .to_scalar::<f32>()? as f64;
            }
            let (_, sl, _) = ys.dims3()?;
            let logits = self
                .model
                .decoder
                .final_linear(&ys.i((..1, sl - 1..))?)?
                .i(0)?
                .i(0)?;
            let logits = logits.broadcast_add(&self.suppress)?;
            let next = if t > 0.0 {
                let prs = candle_nn::ops::softmax(&(&logits / t)?, 0)?.to_vec1::<f32>()?;
                let distr = WeightedIndex::new(&prs)?;
                distr.sample(&mut self.rng) as u32
            } else {
                logits
                    .to_vec1::<f32>()?
                    .into_iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(&b.1))
                    .map(|(i, _)| i as u32)
                    .unwrap_or(self.eot)
            };
            let prob = candle_nn::ops::softmax(&logits, D::Minus1)?
                .i(next as usize)?
                .to_scalar::<f32>()? as f64;
            tokens.push(next);
            if next == self.eot || tokens.len() > self.model.config.max_target_positions {
                break;
            }
            sum_logprob += prob.ln();
        }
        let text = self.tokenizer.decode(&tokens, true).map_err(anyhow::Error::msg)?;
        let avg_logprob = sum_logprob / tokens.len().max(1) as f64;
        Ok((text, avg_logprob, no_speech_prob))
    }

    /// Reintenta con temperaturas crecientes si el resultado es de baja confianza
    /// (como el `decode_with_fallback` de OpenAI/candle).
    fn decode_with_fallback(
        &mut self,
        audio_features: &Tensor,
        lang_token: u32,
    ) -> Result<(String, f64)> {
        let mut last = (String::new(), 0.0, 1.0);
        for (i, &t) in whisper::TEMPERATURES.iter().enumerate() {
            let dr = self.decode(audio_features, lang_token, t)?;
            let needs_fallback = dr.1 < whisper::LOGPROB_THRESHOLD;
            if i == whisper::TEMPERATURES.len() - 1
                || !needs_fallback
                || dr.2 > whisper::NO_SPEECH_THRESHOLD
            {
                return Ok((dr.0, dr.2));
            }
            last = dr;
        }
        Ok((last.0, last.2))
    }

    /// Transcribe PCM f32 mono 16 kHz → (código de idioma, texto). Recorre ventanas
    /// de 30 s con fallback de temperatura. Salta segmentos sin voz.
    pub fn transcribe(&mut self, pcm: &[f32]) -> Result<(String, String)> {
        if pcm.is_empty() {
            return Ok((String::new(), String::new()));
        }
        let mel_v = whisper::audio::pcm_to_mel(&self.model.config, pcm, &self.mel_filters);
        let n_mel = self.model.config.num_mel_bins;
        let mel = Tensor::from_vec(mel_v.clone(), (1, n_mel, mel_v.len() / n_mel), &self.device)?;

        let (lang_code, lang_token) = self.detect_language(&mel)?;
        let (_, _, frames) = mel.dims3()?;
        let mut seek = 0usize;
        let mut full = String::new();
        while seek < frames {
            let seg = usize::min(frames - seek, whisper::N_FRAMES);
            let mel_seg = mel.narrow(2, seek, seg)?;
            seek += seg;
            let af = self.model.encoder.forward(&mel_seg, true)?;
            let (text, no_speech_prob) = self.decode_with_fallback(&af, lang_token)?;
            if no_speech_prob > whisper::NO_SPEECH_THRESHOLD {
                continue; // segmento sin voz → no aportar ruido
            }
            if !text.trim().is_empty() {
                full.push_str(text.trim());
                full.push(' ');
            }
        }
        Ok((lang_code.to_string(), full.trim().to_string()))
    }
}
