//! Spike IA — Fase 1 (búsqueda semántica de imágenes), de-riesgo del stack ML.
//!
//! Corre un modelo CLIP/SigLIP **multilingüe** sobre los thumbnails JPEG que ya
//! están cacheados dentro de un catálogo `.dccat` y los rankea por una query de
//! texto en español ("perros", "árboles", "la playa"…). Prueba, sobre datos
//! REALES y sin re-montar discos, que la búsqueda por contenido funciona antes
//! de cablear nada en la app.
//!
//! Gateado detrás de la feature `ai` (apagada por default → no infla el build
//! normal ni el instalador), igual que los probes detrás de `tools`.
//!
//! Uso:
//!   cargo run --release --features ai --bin embed_probe -- ~/Dropbox/catalog.dccat
//!   cargo run --release --features ai --bin embed_probe -- ~/Dropbox/catalog.dccat perros "árboles" "la playa"
//!   PROBE_LIMIT=500 cargo run --release --features ai --bin embed_probe -- <catalog>
//!
//! La primera corrida descarga el modelo (~400 MB) a ~/.cache/huggingface y lo
//! cachea; las siguientes son offline.

// BLAS de Apple (feature `accel`): el `extern crate` fuerza al linker a incluir
// la librería para que candle use Accelerate en el matmul (CPU rápido).
#[cfg(feature = "accel")]
extern crate accelerate_src;

use anyhow::{bail, Context, Result};
use candle_core::{DType, Device, Tensor, D};
use candle_nn::VarBuilder;
use candle_transformers::models::siglip;
use rusqlite::Connection;
use tokenizers::Tokenizer;

/// Modelo por defecto: SigLIP 2 base patch16-256 (multilingüe vía tokenizer Gemma,
/// más fuerte que el v1). Override con `PROBE_MODEL=google/siglip-base-patch16-256-multilingual`.
/// ~370-450 MB en safetensors f32; se cachea en ~/.cache/huggingface.
const DEFAULT_REPO: &str = "google/siglip2-base-patch16-256";

/// Plantilla de prompt zero-shot de SigLIP. La query reemplaza `{}`. Mejora
/// bastante el ranking vs palabra suelta. Override con PROBE_TEMPLATE (ej "{}").
const DEFAULT_TEMPLATE: &str = "una foto de {}";

/// Queries por defecto (si no pasás ninguna) — conceptos comunes que CLIP maneja bien.
const DEFAULT_QUERIES: &[&str] = &[
    "perros",
    "gatos",
    "árboles",
    "la playa",
    "una persona",
    "comida",
    "un auto",
    "código en una pantalla",
    "montañas",
    "un atardecer",
];

/// Normaliza L2 sobre la última dimensión (para que el matmul dé coseno).
fn l2_normalize(t: &Tensor) -> Result<Tensor> {
    let norm = t.sqr()?.sum_keepdim(D::Minus1)?.sqrt()?;
    Ok(t.broadcast_div(&norm)?)
}

/// JPEG/PNG en bytes → tensor [3, size, size] f32 en rango [-1, 1] (preproc SigLIP).
fn image_to_tensor(bytes: &[u8], size: usize, dev: &Device) -> Result<Tensor> {
    let img = image::load_from_memory(bytes)?
        .resize_to_fill(
            size as u32,
            size as u32,
            image::imageops::FilterType::Triangle,
        )
        .to_rgb8();
    let data = img.into_raw();
    let t = Tensor::from_vec(data, (size, size, 3), dev)?
        .permute((2, 0, 1))?
        .to_dtype(DType::F32)?
        .affine(2.0 / 255.0, -1.0)?;
    Ok(t)
}

fn main() -> Result<()> {
    let mut argv = std::env::args().skip(1);
    let catalog = argv
        .next()
        .context("uso: embed_probe <catalog.dccat> [query...]")?;
    let queries: Vec<String> = {
        let rest: Vec<String> = argv.collect();
        if rest.is_empty() {
            DEFAULT_QUERIES.iter().map(|s| s.to_string()).collect()
        } else {
            rest
        }
    };
    let limit: i64 = std::env::var("PROBE_LIMIT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300);

    // 1) Thumbnails ya cacheados en el catálogo (offline, no necesita el disco).
    let conn = Connection::open(&catalog).context("no se pudo abrir el catálogo")?;
    let mut stmt = conn.prepare(
        "SELECT t.entry_id, e.name, e.ext, t.png \
         FROM thumbnails t JOIN entries e ON e.id = t.entry_id \
         WHERE e.is_folder = 0 LIMIT ?1",
    )?;
    let rows = stmt.query_map([limit], |r| {
        Ok((
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Vec<u8>>(3)?,
        ))
    })?;
    let mut names = Vec::new();
    let mut blobs = Vec::new();
    for row in rows {
        let (name, ext, png) = row?;
        let label = match ext {
            Some(e) => format!("{name}  [.{e}]"),
            None => name,
        };
        names.push(label);
        blobs.push(png);
    }
    if blobs.is_empty() {
        bail!(
            "el catálogo no tiene thumbnails cacheados. Abrí la app, generá previews \
             (galería / inspector) sobre algunas imágenes o videos, y volvé a correr."
        );
    }
    eprintln!("thumbnails a indexar: {}", blobs.len());

    // 2) Modelo (descarga + caché en ~/.cache/huggingface la 1ª vez).
    let repo = std::env::var("PROBE_MODEL").unwrap_or_else(|_| DEFAULT_REPO.to_string());
    let device = Device::Cpu;
    eprintln!("cargando modelo {repo} (la 1ª vez descarga el safetensors)…");
    let api = hf_hub::api::sync::Api::new()?.model(repo.clone());
    let model_file = api.get("model.safetensors")?;
    let config_file = api.get("config.json")?;
    let tok_file = api.get("tokenizer.json")?;
    let config: siglip::Config = serde_json::from_slice(&std::fs::read(config_file)?)?;
    let tokenizer = Tokenizer::from_file(tok_file).map_err(anyhow::Error::msg)?;
    let image_size = config.vision_config.image_size;
    let max_len = config.text_config.max_position_embeddings;
    // El config.json mínimo no trae pad_token_id; lo tomamos del tokenizer
    // (gemma <pad>=0) y si no, del default del config. Override con PROBE_PAD.
    let pad_id = std::env::var("PROBE_PAD")
        .ok()
        .and_then(|s| s.parse().ok())
        .or_else(|| tokenizer.token_to_id("<pad>"))
        .unwrap_or(config.text_config.pad_token_id);
    let template = std::env::var("PROBE_TEMPLATE").unwrap_or_else(|_| DEFAULT_TEMPLATE.to_string());
    eprintln!("prompt template: \"{template}\"  ·  pad_id: {pad_id}  ·  max_len: {max_len}");
    let vb =
        unsafe { VarBuilder::from_mmaped_safetensors(&[model_file], DType::F32, &device)? };
    let model = siglip::Model::new(&config, vb)?;

    // 3) Features de imagen (batched, para no cargar todo en RAM/CPU de una).
    let t0 = std::time::Instant::now();
    let batch = 16usize;
    let mut feats: Vec<Tensor> = Vec::new();
    let mut done = 0usize;
    for chunk in blobs.chunks(batch) {
        let mut ts = Vec::with_capacity(chunk.len());
        for b in chunk {
            // Saltea un thumbnail corrupto sin abortar todo el probe.
            match image_to_tensor(b, image_size, &device) {
                Ok(t) => ts.push(t),
                Err(e) => {
                    eprintln!("  (saltado: {e})");
                }
            }
        }
        if ts.is_empty() {
            continue;
        }
        let imgs = Tensor::stack(&ts, 0)?;
        feats.push(l2_normalize(&model.get_image_features(&imgs)?)?);
        done += chunk.len();
        eprintln!("  imágenes {done}/{}", blobs.len());
    }
    let img_feats = Tensor::cat(&feats, 0)?; // [N, dim]
    eprintln!("embeddings de imagen en {:?}", t0.elapsed());

    // 4) Features de texto (una pasada).
    let mut toks: Vec<Vec<u32>> = Vec::new();
    for q in &queries {
        let prompt = template.replace("{}", &q.to_lowercase());
        let enc = tokenizer
            .encode(prompt, true)
            .map_err(anyhow::Error::msg)?;
        let mut ids = enc.get_ids().to_vec();
        ids.truncate(max_len);
        while ids.len() < max_len {
            ids.push(pad_id);
        }
        toks.push(ids);
    }
    let input_ids = Tensor::new(toks, &device)?;
    let txt_feats = l2_normalize(&model.get_text_features(&input_ids)?)?; // [Q, dim]

    // 5) Coseno [Q, N] y ranking por query.
    let sims = txt_feats.matmul(&img_feats.t()?)?.to_vec2::<f32>()?;
    for (qi, q) in queries.iter().enumerate() {
        let mut scored: Vec<(f32, &String)> =
            sims[qi].iter().cloned().zip(names.iter()).collect();
        scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
        println!("\n== \"{q}\" ==");
        for (s, n) in scored.iter().take(8) {
            println!("  {s:.4}  {n}");
        }
    }
    Ok(())
}
