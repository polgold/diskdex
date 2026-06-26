//! Importador del formato `.dcmf` de DiskCatalogMaker (Fujiwara Software).
//!
//! Especificación: sección 6 del prompt (ingeniería-reversada y validada).
//! - Contenedor: header de 10 bytes + secuencia de bloques `[u32 BE ulen][stream zlib]`.
//! - Lectura robusta: escanear el archivo buscando cabeceras zlib y validarlas
//!   (no leer secuencialmente, porque hay regiones que no son zlib entre grupos).
//! - Por disco hay (en orden): tabla de nombres, tabla de registros (24 B),
//!   tabla de atributos de carpeta (24 B, opcional), tabla de atributos de archivo (40 B).
//!
//! Esta es la versión nativa (Rust) recomendada por el prompt para el archivo real
//! de 164 MB: infla en streaming con `flate2` y evita cargar ~1 GB en JS.

use flate2::{Decompress, FlushDecompress, Status};
use serde::Serialize;

/// Segundos entre la época HFS (1904-01-01) y la época Unix (1970-01-01).
const HFS_OFFSET: i64 = 2_082_844_800;

/// Unix seconds de 1980-01-01. Fechas anteriores se consideran desconocidas.
const MIN_VALID_UNIX: i64 = 315_532_800;

/// Tope defensivo para el tamaño descomprimido de un bloque. Los bloques reales
/// están muy por debajo (decenas de MB); esto evita que 4 bytes de basura antes
/// de un `0x78` casual disparen una asignación gigante (OOM).
const MAX_BLOCK_ULEN: usize = 600 * 1024 * 1024;

/// Flags de tipo en el campo f2 del registro.
const FLAG_VOLUME: u32 = 0x40000;
const FLAG_FOLDER: u32 = 0x20000;

#[derive(Debug, Clone, Serialize)]
pub struct DcmfEntry {
    pub name: String,
    /// Índice local dentro del disco; -1 para la raíz del volumen.
    pub parent: i32,
    pub is_folder: bool,
    pub is_volume: bool,
    pub size_logical: u64,
    pub size_physical: u64,
    /// Unix seconds (0 = desconocida).
    pub created: i64,
    pub modified: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DcmfDisk {
    pub name: String,
    pub entries: Vec<DcmfEntry>,
}

#[inline]
fn be_u16(b: &[u8], o: usize) -> u16 {
    ((b[o] as u16) << 8) | (b[o + 1] as u16)
}

#[inline]
fn be_u32(b: &[u8], o: usize) -> u32 {
    ((b[o] as u32) << 24) | ((b[o + 1] as u32) << 16) | ((b[o + 2] as u32) << 8) | (b[o + 3] as u32)
}

#[inline]
fn be_u64(b: &[u8], o: usize) -> u64 {
    ((b[o] as u64) << 56)
        | ((b[o + 1] as u64) << 48)
        | ((b[o + 2] as u64) << 40)
        | ((b[o + 3] as u64) << 32)
        | ((b[o + 4] as u64) << 24)
        | ((b[o + 5] as u64) << 16)
        | ((b[o + 6] as u64) << 8)
        | (b[o + 7] as u64)
}

/// Convierte segundos HFS a Unix seconds. Devuelve 0 si es desconocida o fuera de rango.
#[inline]
fn hfs_to_unix(hfs: u32) -> i64 {
    if hfs == 0 {
        return 0;
    }
    let unix = hfs as i64 - HFS_OFFSET;
    if unix < MIN_VALID_UNIX {
        0
    } else {
        unix
    }
}

struct Block {
    ulen: usize,
    data: Vec<u8>,
}

/// Escanea el buffer buscando streams zlib válidos cuyo tamaño descomprimido
/// coincida con el `u32 BE` que los precede. Avanza por los bytes consumidos
/// de cada stream válido y por 1 byte cuando no encuentra uno.
fn read_blocks(buf: &[u8]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let n = buf.len();
    if n < 2 {
        return blocks;
    }
    let mut i = 0usize;
    while i < n - 1 {
        let b0 = buf[i];
        let b1 = buf[i + 1];
        // Cabecera zlib: 0x78 seguido de 0x01 (sin compresión/baja), 0x9c (default) o 0xda (mejor).
        if b0 == 0x78 && (b1 == 0x01 || b1 == 0x9c || b1 == 0xda) && i >= 4 {
            let ulen = be_u32(buf, i - 4) as usize;
            if ulen > 0 && ulen <= MAX_BLOCK_ULEN {
                if let Some((data, consumed)) = try_inflate(&buf[i..], ulen) {
                    blocks.push(Block { ulen, data });
                    // Avanzar exactamente por los bytes comprimidos consumidos.
                    i += consumed.max(1);
                    continue;
                }
            }
        }
        i += 1;
    }
    blocks
}

/// Intenta inflar un único stream zlib que empieza en `input`, esperando `ulen`
/// bytes de salida. Devuelve `(salida, bytes_comprimidos_consumidos)` si infló OK
/// y la longitud coincide exactamente.
fn try_inflate(input: &[u8], ulen: usize) -> Option<(Vec<u8>, usize)> {
    let mut dec = Decompress::new(true); // true = esperar header zlib
    let mut out: Vec<u8> = Vec::with_capacity(ulen);
    match dec.decompress_vec(input, &mut out, FlushDecompress::Finish) {
        Ok(Status::StreamEnd) if out.len() == ulen => Some((out, dec.total_in() as usize)),
        _ => None,
    }
}

/// Parsea un bloque como tabla de nombres. Cada entrada:
/// `[4 bytes (normalmente 0)] [u16 BE = cant chars] [chars UTF-16 BE]`.
/// Devuelve `None` si el bloque no parsea limpio hasta el final.
fn parse_names(out: &[u8]) -> Option<Vec<String>> {
    let mut res = Vec::new();
    let mut pos = 0usize;
    let len = out.len();
    while pos + 6 <= len {
        pos += 4; // 4 bytes reservados
        let n = be_u16(out, pos) as usize;
        pos += 2;
        if pos + n * 2 > len {
            return None;
        }
        let mut units: Vec<u16> = Vec::with_capacity(n);
        for k in 0..n {
            units.push(be_u16(out, pos + k * 2));
        }
        pos += n * 2;
        res.push(String::from_utf16_lossy(&units));
    }
    if pos == len {
        Some(res)
    } else {
        None
    }
}

/// Parsea un buffer `.dcmf` completo y devuelve los discos con sus entradas.
pub fn import_dcmf(buf: &[u8]) -> Vec<DcmfDisk> {
    let blocks = read_blocks(buf);
    let parsed_names: Vec<Option<Vec<String>>> =
        blocks.iter().map(|b| parse_names(&b.data)).collect();

    let mut disks = Vec::new();

    for ni in 0..blocks.len() {
        let names = match &parsed_names[ni] {
            Some(n) if !n.is_empty() => n,
            _ => continue,
        };
        let count = names.len();

        // Tabla de registros (24 B): buscar entre los bloques cercanos uno con ulen == count*24.
        let want_recs = count.checked_mul(24);
        let mut ri: isize = -1;
        if let Some(want) = want_recs {
            let end = (ni + 6).min(blocks.len());
            for j in (ni + 1)..end {
                if blocks[j].ulen == want {
                    ri = j as isize;
                    break;
                }
            }
        }
        if ri < 0 {
            continue;
        }
        let recs = &blocks[ri as usize].data;

        let mut parent = vec![-1i32; count];
        let mut type_code = vec![0u8; count]; // 0=archivo, 1=carpeta, 2=volumen
        let mut file_seq = vec![0u32; count];
        let mut max_file_seq: u32 = 0;

        for k in 0..count {
            let base = k * 24;
            let f1 = be_u32(recs, base + 4); // padre
            let f2 = be_u32(recs, base + 8); // flags de tipo
            let f5 = be_u32(recs, base + 20); // file-seq
            parent[k] = if k == 0 {
                -1
            } else if (f1 as usize) < count {
                f1 as i32
            } else {
                -1
            };
            type_code[k] = if f2 & FLAG_VOLUME != 0 {
                2
            } else if f2 & FLAG_FOLDER != 0 {
                1
            } else {
                0
            };
            file_seq[k] = f5;
            if f5 > max_file_seq {
                max_file_seq = f5;
            }
        }

        // Tabla de atributos de archivo (40 B): ulen == (maxFileSeq+1)*40.
        let want_attrs = (max_file_seq as usize + 1).checked_mul(40);
        let mut fa: isize = -1;
        if let Some(want) = want_attrs {
            let end = (ni + 6).min(blocks.len());
            for j in (ni + 1)..end {
                if (j as isize) != ri && blocks[j].ulen == want {
                    fa = j as isize;
                    break;
                }
            }
        }
        let attrs: Option<&Vec<u8>> = if fa >= 0 {
            Some(&blocks[fa as usize].data)
        } else {
            None
        };

        let mut entries = Vec::with_capacity(count);
        for k in 0..count {
            let mut size_logical = 0u64;
            let mut size_physical = 0u64;
            let mut created = 0i64;
            let mut modified = 0i64;

            if let Some(adv) = attrs {
                if type_code[k] == 0 && file_seq[k] > 0 {
                    let base = file_seq[k] as usize * 40;
                    if base + 40 <= adv.len() {
                        created = hfs_to_unix(be_u32(adv, base));
                        modified = hfs_to_unix(be_u32(adv, base + 4));
                        size_logical = be_u64(adv, base + 24);
                        size_physical = be_u64(adv, base + 32);
                    }
                }
            }

            entries.push(DcmfEntry {
                name: names[k].clone(),
                parent: parent[k],
                is_folder: type_code[k] != 0,
                is_volume: type_code[k] == 2,
                size_logical,
                size_physical,
                created,
                modified,
            });
        }

        disks.push(DcmfDisk {
            name: names[0].clone(),
            entries,
        });
    }

    disks
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    /// Comprime `data` como stream zlib y lo prefija con su `u32 BE` de longitud
    /// descomprimida, igual que un bloque `.dcmf`.
    fn make_block(data: &[u8]) -> Vec<u8> {
        let mut enc = ZlibEncoder::new(Vec::new(), Compression::default());
        enc.write_all(data).unwrap();
        let compressed = enc.finish().unwrap();
        let mut out = Vec::new();
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(&compressed);
        out
    }

    /// Construye una tabla de nombres a partir de strings.
    fn make_names(names: &[&str]) -> Vec<u8> {
        let mut out = Vec::new();
        for name in names {
            let units: Vec<u16> = name.encode_utf16().collect();
            out.extend_from_slice(&[0u8; 4]); // 4 bytes reservados
            out.extend_from_slice(&(units.len() as u16).to_be_bytes());
            for u in units {
                out.extend_from_slice(&u.to_be_bytes());
            }
        }
        out
    }

    /// Un registro de 24 bytes = 6 × u32 BE.
    fn rec(sibling: u32, parent: u32, type_flag: u32, f3: u32, folder_seq: u32, file_seq: u32) -> [u8; 24] {
        let mut r = [0u8; 24];
        r[0..4].copy_from_slice(&sibling.to_be_bytes());
        r[4..8].copy_from_slice(&parent.to_be_bytes());
        r[8..12].copy_from_slice(&type_flag.to_be_bytes());
        r[12..16].copy_from_slice(&f3.to_be_bytes());
        r[16..20].copy_from_slice(&folder_seq.to_be_bytes());
        r[20..24].copy_from_slice(&file_seq.to_be_bytes());
        r
    }

    /// Un registro de atributos de archivo de 40 bytes.
    fn file_attr(created_hfs: u32, modified_hfs: u32, size_logical: u64, size_physical: u64) -> [u8; 40] {
        let mut a = [0u8; 40];
        a[0..4].copy_from_slice(&created_hfs.to_be_bytes());
        a[4..8].copy_from_slice(&modified_hfs.to_be_bytes());
        a[8..12].copy_from_slice(&1u32.to_be_bytes()); // flags
        a[24..32].copy_from_slice(&size_logical.to_be_bytes());
        a[32..40].copy_from_slice(&size_physical.to_be_bytes());
        a
    }

    /// Construye un `.dcmf` sintético de un disco:
    /// volumen "SF28" → carpeta "CLIP" → archivo "C0001.MP4" (4.25 GB).
    fn build_synthetic_dcmf() -> (Vec<u8>, u64) {
        // Nodos (paralelos): 0=volumen, 1=carpeta, 2=archivo.
        let names = make_names(&["SF28", "CLIP", "C0001.MP4"]);

        // 4.25 GB en bytes = 4.25 * 1024^3.
        let big_size: u64 = 4_563_402_752; // ≈ 4.25 GiB, > 2^32 (prueba 64-bit)
        let phys_size: u64 = 4_563_406_848;

        // Fecha de creación: 2023-06-01 00:00:00 UTC en HFS = unix + HFS_OFFSET.
        let unix_created: i64 = 1_685_577_600;
        let created_hfs = (unix_created + HFS_OFFSET) as u32;
        let modified_hfs = created_hfs + 3600;

        // Registros: file_seq 0 = volumen; el archivo usa file_seq 1.
        let mut recs = Vec::new();
        recs.extend_from_slice(&rec(0xFFFF_FFFF, 0, FLAG_VOLUME | FLAG_FOLDER, 0, 0, 0)); // volumen
        recs.extend_from_slice(&rec(0xFFFF_FFFF, 0, FLAG_FOLDER, 0, 1, 0)); // carpeta CLIP, padre=0
        recs.extend_from_slice(&rec(0xFFFF_FFFF, 1, 0x10000, 0, 0, 1)); // archivo, padre=1, file_seq=1

        // Atributos de archivo: índice 0 (volumen) + índice 1 (el archivo). maxFileSeq=1.
        let mut attrs = Vec::new();
        attrs.extend_from_slice(&file_attr(0, 0, 0, 0)); // índice 0
        attrs.extend_from_slice(&file_attr(created_hfs, modified_hfs, big_size, phys_size)); // índice 1

        let mut buf = Vec::new();
        buf.extend_from_slice(&[0u8; 10]); // header de 10 bytes
        buf.extend_from_slice(&make_block(&names));
        buf.extend_from_slice(&[0xAB, 0xCD, 0xEF]); // región no-zlib intermedia (caso real)
        buf.extend_from_slice(&make_block(&recs));
        buf.extend_from_slice(&make_block(&attrs));
        (buf, big_size)
    }

    #[test]
    fn reads_blocks_and_skips_non_zlib_regions() {
        let (buf, _big) = build_synthetic_dcmf();
        let blocks = read_blocks(&buf);
        // names + recs + attrs = 3 bloques, sin falsos positivos por la basura intermedia.
        assert_eq!(blocks.len(), 3);
    }

    #[test]
    fn imports_single_disk_tree() {
        let (buf, _big_size) = build_synthetic_dcmf();
        let disks = import_dcmf(&buf);
        assert_eq!(disks.len(), 1);
        let d = &disks[0];
        assert_eq!(d.name, "SF28");
        assert_eq!(d.entries.len(), 3);

        let vol = &d.entries[0];
        assert!(vol.is_volume);
        assert!(vol.is_folder);
        assert_eq!(vol.parent, -1);

        let folder = &d.entries[1];
        assert!(folder.is_folder);
        assert!(!folder.is_volume);
        assert_eq!(folder.parent, 0);

        let file = &d.entries[2];
        assert!(!file.is_folder);
        assert_eq!(file.name, "C0001.MP4");
        assert_eq!(file.parent, 1);
    }

    #[test]
    fn preserves_64bit_size_over_4gb() {
        // Verifica que NO se trunca a 32 bits (el bug clásico).
        let (buf, big_size) = build_synthetic_dcmf();
        let disks = import_dcmf(&buf);
        let file = &disks[0].entries[2];
        assert!(big_size > u32::MAX as u64, "fixture debe superar 2^32");
        assert_eq!(file.size_logical, big_size);
        assert!(file.size_physical >= big_size);
    }

    #[test]
    fn converts_hfs_dates_to_unix() {
        let (buf, _) = build_synthetic_dcmf();
        let disks = import_dcmf(&buf);
        let file = &disks[0].entries[2];
        assert_eq!(file.created, 1_685_577_600);
        assert_eq!(file.modified, 1_685_577_600 + 3600);
    }

    #[test]
    fn folders_have_zero_size_and_unknown_dates() {
        let (buf, _) = build_synthetic_dcmf();
        let disks = import_dcmf(&buf);
        let folder = &disks[0].entries[1];
        assert_eq!(folder.size_logical, 0);
        assert_eq!(folder.created, 0);
    }

    #[test]
    fn hfs_conversion_handles_out_of_range() {
        assert_eq!(hfs_to_unix(0), 0); // desconocida
        assert_eq!(hfs_to_unix(1), 0); // año 1904 → < 1980 → desconocida
        // 2000-01-01 es válido.
        let unix_2000 = 946_684_800i64;
        assert_eq!(hfs_to_unix((unix_2000 + HFS_OFFSET) as u32), unix_2000);
    }

    #[test]
    fn rejects_garbage_ulen_without_panic() {
        // u32 enorme antes de un 0x78 casual no debe asignar ni romper.
        let mut buf = vec![0xFF, 0xFF, 0xFF, 0xFF, 0x78, 0x9c, 0x00, 0x00];
        buf.extend_from_slice(&[0u8; 16]);
        let blocks = read_blocks(&buf);
        assert!(blocks.is_empty());
    }
}
