//! Ingesta del reporte de discos de DiskCatalogMaker (exportado como texto/PDF).
//!
//! DiskCatalogMaker no persiste el espacio libre en el `.dcmf` de forma
//! recuperable, pero sí lo muestra e imprime. Este módulo parsea ese reporte y
//! actualiza `free_space`/`capacity`/`kind` de cada disco por nombre, para tener
//! el uso de todos los discos sin reconectarlos uno por uno.
//!
//! Formato de cada línea (el nombre puede tener espacios y la fecha es opcional):
//!   NOMBRE  SIZE u  FREE u  KIND [FECHA]  COUNT items  CAPACITY u
//! p.ej. `SFBACKUP8 13,66 TB 6,34 TB USB HD 131.802 items 20 TB`
//!       `SF38 3,28 TB 1,72 TB USB HD 18/07/2026 9.869 items 5 TB`

use rusqlite::{params, Connection};

/// Una fila parseada del reporte.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportRow {
    pub name: String,
    pub free_bytes: i64,
    pub capacity_bytes: i64,
    pub kind: String,
    /// Total de ítems que reporta DCM: se usa para desempatar discos homónimos.
    pub count: i64,
}

/// Convierte "13,66 TB" / "500,06 GB" / "771 MB" a bytes (base 1024, como el
/// display de DCM). La coma es separador decimal (locale es-AR).
fn parse_size(value: &str, unit: &str) -> Option<i64> {
    let v: f64 = value.replace(',', ".").parse().ok()?;
    let mult = match unit {
        "TB" => 1024f64.powi(4),
        "GB" => 1024f64.powi(3),
        "MB" => 1024f64.powi(2),
        "KB" => 1024f64,
        "B" => 1.0,
        _ => return None,
    };
    Some((v * mult) as i64)
}

fn is_unit(t: &str) -> bool {
    matches!(t, "TB" | "GB" | "MB" | "KB" | "B")
}

/// Parsea una línea del reporte desde la DERECHA (así el nombre, que puede tener
/// espacios, es simplemente "lo que sobra"). Devuelve None si no encaja.
fn parse_line(line: &str) -> Option<ReportRow> {
    // La cabecera y el pie del reporte se saltean.
    let low = line.trim();
    if low.is_empty()
        || low.starts_with("Name ")
        || low.starts_with("Catalog")
        || low.contains("items, total")
        || low.contains("Page ")
    {
        return None;
    }
    let t: Vec<&str> = low.split_whitespace().collect();
    if t.len() < 7 {
        return None;
    }
    // Desde la derecha: CAPACITY = [.. val unit]
    let n = t.len();
    if !is_unit(t[n - 1]) {
        return None;
    }
    let capacity_bytes = parse_size(t[n - 2], t[n - 1])?;
    // "items" antes de la capacidad.
    if t[n - 3] != "items" {
        return None;
    }
    let count: i64 = t[n - 4].replace('.', "").parse().ok()?;
    // Puede venir una fecha (dd/mm/yyyy) antes del count.
    let mut i = n - 5;
    if t[i].len() == 10 && t[i].as_bytes()[2] == b'/' && t[i].as_bytes()[5] == b'/' {
        i -= 1; // saltar la fecha
    }
    // KIND: "USB HD" o "internal disk" (dos tokens) o uno solo ("SSD"…).
    // Detectamos hacia atrás hasta toparnos con la unidad del "free".
    // free = [val unit] justo antes del kind.
    // Buscamos el índice de la unidad del free: es el primer `is_unit` yendo a la
    // izquierda desde `i` que tenga un número parseable a su izquierda.
    let mut kind_end = i; // último token del kind (inclusive)
    // Encontrar la unidad de free: escaneamos a la izquierda.
    let mut free_unit_idx = None;
    let mut j = kind_end;
    loop {
        if is_unit(t[j]) && j >= 1 && t[j - 1].replace(',', ".").parse::<f64>().is_ok() {
            free_unit_idx = Some(j);
            break;
        }
        if j == 0 {
            break;
        }
        j -= 1;
    }
    let fu = free_unit_idx?;
    let free_bytes = parse_size(t[fu - 1], t[fu])?;
    // El kind es lo que hay entre la unidad de free (fu) y `i` (o la fecha).
    let kind = t[(fu + 1)..=kind_end].join(" ");
    let _ = &mut kind_end;
    // Antes del free viene el size: [val@(fu-3)  unit@(fu-2)  free_val@(fu-1)  free_unit@fu].
    if fu < 3 || !is_unit(t[fu - 2]) {
        return None;
    }
    // El nombre es todo lo anterior al valor del size.
    let name = t[0..(fu - 3)].join(" ");
    if name.is_empty() {
        return None;
    }
    Some(ReportRow {
        name,
        free_bytes,
        capacity_bytes,
        kind,
        count,
    })
}

/// Parsea el reporte completo (una fila por línea válida).
pub fn parse_report(text: &str) -> Vec<ReportRow> {
    text.lines().filter_map(parse_line).collect()
}

/// Resultado de aplicar el reporte al catálogo.
#[derive(Debug, Default, serde::Serialize)]
pub struct ReportApply {
    /// Discos actualizados (nombre).
    pub updated: Vec<String>,
    /// Filas del reporte sin disco correspondiente en el catálogo.
    pub unmatched: Vec<String>,
}

/// Aplica el reporte: por cada fila, actualiza el disco de igual nombre. Para
/// nombres duplicados (p.ej. dos "SF34") empareja por el total de ítems más
/// cercano, uno a uno, para no pisar el free del disco equivocado.
pub fn apply_report(conn: &Connection, rows: &[ReportRow]) -> rusqlite::Result<ReportApply> {
    let mut out = ReportApply::default();

    // Discos del catálogo por nombre: (id, item_count), para desempatar.
    let mut by_name: std::collections::HashMap<String, Vec<(i64, i64)>> =
        std::collections::HashMap::new();
    {
        let mut stmt = conn.prepare("SELECT id, name, file_count + folder_count FROM disks")?;
        let it = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, i64>(2)?))
        })?;
        for row in it {
            let (id, name, cnt) = row?;
            by_name.entry(name).or_default().push((id, cnt));
        }
    }

    for row in rows {
        let candidates = match by_name.get_mut(&row.name) {
            Some(c) if !c.is_empty() => c,
            _ => {
                out.unmatched.push(row.name.clone());
                continue;
            }
        };
        // Elegir el candidato con item-count más cercano y consumirlo (uno a uno).
        let pick = candidates
            .iter()
            .enumerate()
            .min_by_key(|(_, (_, cnt))| (cnt - row.count).abs())
            .map(|(idx, (id, _))| (idx, *id));
        if let Some((idx, id)) = pick {
            conn.execute(
                "UPDATE disks SET free_space = ?1, capacity = ?2, \
                 kind = COALESCE(NULLIF(?3,''), kind) WHERE id = ?4",
                params![row.free_bytes, row.capacity_bytes, row.kind, id],
            )?;
            out.updated.push(row.name.clone());
            candidates.remove(idx);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_lines_with_and_without_date_and_spaced_names() {
        // Sin fecha, capacidad en TB redondo.
        let r = parse_line("SFBACKUP8 13,66 TB 6,34 TB USB HD 131.802 items 20 TB").unwrap();
        assert_eq!(r.name, "SFBACKUP8");
        assert_eq!(r.kind, "USB HD");
        assert_eq!(r.count, 131_802);
        assert_eq!(r.capacity_bytes, 20 * 1024i64.pow(4));
        // 6,34 TB
        assert_eq!(r.free_bytes, (6.34 * 1024f64.powi(4)) as i64);

        // Con fecha.
        let r = parse_line("SF38 3,28 TB 1,72 TB USB HD 18/07/2026 9.869 items 5 TB").unwrap();
        assert_eq!(r.name, "SF38");
        assert_eq!(r.count, 9_869);

        // Nombre con espacio + capacidad en GB con decimales.
        let r = parse_line("SF20 1 3,7 TB 295,75 GB USB HD 31/08/2021 176.445 items 4 TB").unwrap();
        assert_eq!(r.name, "SF20 1");
        assert_eq!(r.free_bytes, (295.75 * 1024f64.powi(3)) as i64);

        // Nombre de dos palabras + kind "internal disk".
        let r = parse_line("Macintosh HD 970,7 GB 29,54 GB internal disk 01/01/2020 2.442.007 items 1 TB").unwrap();
        assert_eq!(r.name, "Macintosh HD");
        assert_eq!(r.kind, "internal disk");
        assert_eq!(r.count, 2_442_007);

        // Free en MB.
        let r = parse_line("MIRROR 2 498,99 GB 771 MB USB HD 06/12/2013 15.347 items 499,76 GB").unwrap();
        assert_eq!(r.name, "MIRROR 2");
        assert_eq!(r.free_bytes, 771 * 1024i64.pow(2));
    }

    #[test]
    fn skips_header_and_footer() {
        assert!(parse_line("Name Size Free Kind Date Modified Total Count Capacity").is_none());
        assert!(parse_line("54 items, total 6.832.659 items, 54 disks Page 1 of 1").is_none());
        assert!(parse_line("Catalog 27/07/2026, 9:55AM").is_none());
        assert!(parse_line("").is_none());
    }

    #[test]
    fn duplicate_names_matched_by_closest_count() {
        let mut conn = crate::db::open_in_memory().unwrap();
        // Dos discos "SF34" con conteos distintos.
        conn.execute("INSERT INTO disks (name, file_count, folder_count) VALUES ('SF34', 231889, 0)", []).unwrap();
        conn.execute("INSERT INTO disks (name, file_count, folder_count) VALUES ('SF34', 231903, 0)", []).unwrap();
        let rows = vec![
            ReportRow { name: "SF34".into(), free_bytes: 100, capacity_bytes: 5 * 1024i64.pow(4), kind: "USB HD".into(), count: 231_903 },
            ReportRow { name: "SF34".into(), free_bytes: 200, capacity_bytes: 5 * 1024i64.pow(4), kind: "USB HD".into(), count: 231_889 },
        ];
        let res = apply_report(&conn, &rows).unwrap();
        assert_eq!(res.updated.len(), 2);
        // Cada fila fue al disco de conteo más cercano.
        let free_of = |cnt: i64| -> i64 {
            conn.query_row("SELECT free_space FROM disks WHERE file_count = ?1", params![cnt], |r| r.get(0)).unwrap()
        };
        assert_eq!(free_of(231_903), 100);
        assert_eq!(free_of(231_889), 200);
    }

    #[test]
    fn reports_unmatched() {
        let conn = crate::db::open_in_memory().unwrap();
        let rows = vec![ReportRow { name: "FANTASMA".into(), free_bytes: 1, capacity_bytes: 1, kind: "USB HD".into(), count: 1 }];
        let res = apply_report(&conn, &rows).unwrap();
        assert_eq!(res.unmatched, vec!["FANTASMA"]);
        assert!(res.updated.is_empty());
    }
}
