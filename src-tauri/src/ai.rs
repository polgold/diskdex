//! IA local (Fase 1): búsqueda semántica de imágenes con SigLIP 2 multilingüe
//! vía candle. Todo en CPU (Accelerate con la feature `accel`), sin servicios
//! externos. El modelo se descarga/cachea en ~/.cache/huggingface la 1ª vez.
//!
//! Gateado por la feature `ai` (apagada por default → no infla el build normal).
//! Validado en el spike `embed_probe`: ver memoria del proyecto.

use anyhow::Result;
use candle_core::{DType, Device, Tensor, D};
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
