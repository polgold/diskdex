//! Reverse-geocoding OFFLINE (C1): mapea coordenadas (lat/lon) a un nombre de
//! lugar legible (ciudad, provincia, país) sin tocar la red ni mandar las
//! ubicaciones a ningún servicio. Usa el dataset de ciudades que bundlea el crate
//! `reverse_geocoder` (GeoNames) + búsqueda por vecino más cercano.
//!
//! El resultado (`gps_place`) se guarda en el catálogo y entra a la búsqueda, así
//! "Jujuy" encuentra los clips grabados ahí.

use reverse_geocoder::ReverseGeocoder;
use std::sync::OnceLock;

/// Geocoder cargado una sola vez (el dataset embebido + el kd-tree son pesados de
/// construir; se reusa entre escaneos).
static GEOCODER: OnceLock<ReverseGeocoder> = OnceLock::new();

/// Nombre de lugar para unas coordenadas: "Ciudad, Provincia, PAÍS" (omite vacíos).
/// Devuelve None si las coordenadas no resuelven a nada razonable.
pub fn place_for(lat: f64, lon: f64) -> Option<String> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let gc = GEOCODER.get_or_init(ReverseGeocoder::new);
    let result = gc.search((lat, lon));
    let r = result.record;
    // name = ciudad, admin1 = provincia/estado, cc = código de país.
    let parts: Vec<&str> = [r.name.as_str(), r.admin1.as_str(), r.cc.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(", "))
    }
}

// ─────────────────────────── Fase de luz / atardecer (C2) ───────────────────────────
//
// "Reuní todos los atardeceres" SIN visión: con GPS + hora de captura se calcula la
// posición del sol (elevación sobre el horizonte) y se clasifica la luz. Cero ML.
// CAVEAT: asume que `captured_at` está en UTC (como lo escribe ffprobe con `Z`). Si la
// cámara guardó hora LOCAL sin zona, el resultado puede correrse — es una heurística.

/// Elevación solar (grados sobre el horizonte) y si el sol está descendiendo (tarde)
/// o ascendiendo (mañana), para `lat`/`lon` (grados) en `unix` (segundos UTC).
/// Algoritmo de baja precisión (J2000): suficiente para clasificar luz/crepúsculo.
fn sun_elevation(lat: f64, lon: f64, unix: i64) -> (f64, bool) {
    let rad = std::f64::consts::PI / 180.0;
    let jd = unix as f64 / 86400.0 + 2440587.5;
    let n = jd - 2451545.0;
    let l = (280.460 + 0.9856474 * n).rem_euclid(360.0);
    let g = (357.528 + 0.9856003 * n).rem_euclid(360.0);
    let lambda = l + 1.915 * (g * rad).sin() + 0.020 * (2.0 * g * rad).sin();
    let eps = 23.439 - 0.0000004 * n;
    let decl = ((eps * rad).sin() * (lambda * rad).sin()).asin(); // rad
    let alpha = ((eps * rad).cos() * (lambda * rad).sin()).atan2((lambda * rad).cos()); // rad
    let gmst = (280.46061837 + 360.98564736629 * n).rem_euclid(360.0);
    let lst = (gmst + lon).rem_euclid(360.0); // grados
    // Hour angle (grados), normalizado a -180..180. >0 = después del mediodía solar.
    let mut h = lst - alpha / rad;
    h = (h + 180.0).rem_euclid(360.0) - 180.0;
    let lat_r = lat * rad;
    let elev = (lat_r.sin() * decl.sin() + lat_r.cos() * decl.cos() * (h * rad).cos()).asin() / rad;
    (elev, h > 0.0)
}

/// Clasifica la luz en palabras buscables (en inglés; el parser traduce
/// "atardecer"→sunset). Devuelve None si faltan coordenadas válidas.
///   elev ≥ 6°  → "day"
///   0..6°      → "golden" + dusk/sunset | dawn/sunrise
///   -6..0°     → "twilight bluehour" + dusk/sunset | dawn/sunrise
///   -18..-6°   → "twilight" + dusk | dawn
///   < -18°     → "night"
pub fn light_phase(lat: f64, lon: f64, unix: i64) -> Option<String> {
    if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    let (elev, descending) = sun_elevation(lat, lon, unix);
    let dir = if descending { "dusk sunset" } else { "dawn sunrise" };
    let phase = if elev >= 6.0 {
        "day".to_string()
    } else if elev >= 0.0 {
        format!("golden {dir}")
    } else if elev >= -6.0 {
        format!("twilight bluehour {dir}")
    } else if elev >= -18.0 {
        format!("twilight {dir}")
    } else {
        "night".to_string()
    };
    Some(phase)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_known_coordinates() {
        // San Salvador de Jujuy, Argentina (-24.1858, -65.2995) → debe mencionar Jujuy.
        let place = place_for(-24.1858, -65.2995).expect("debería resolver");
        assert!(
            place.to_lowercase().contains("jujuy"),
            "esperaba que mencionara Jujuy, obtuve: {place}"
        );
        assert!(place.contains("AR"), "esperaba el país AR, obtuve: {place}");
    }

    #[test]
    fn rejects_out_of_range() {
        assert!(place_for(999.0, 0.0).is_none());
    }

    #[test]
    fn light_phase_day_and_night() {
        // Ecuador, lon 0. 1970-01-01 12:00 UTC = mediodía solar → sol alto → "day".
        let noon = light_phase(0.0, 0.0, 43_200).unwrap();
        assert!(noon.contains("day"), "mediodía debería ser 'day', fue: {noon}");
        // Medianoche en lon 0 → sol bien debajo del horizonte → "night".
        let midnight = light_phase(0.0, 0.0, 0).unwrap();
        assert!(midnight.contains("night"), "medianoche debería ser 'night', fue: {midnight}");
    }

    #[test]
    fn light_phase_detects_a_sunset() {
        // Barremos un día en Buenos Aires (-34.6, -58.4) y buscamos un instante
        // clasificado como atardecer (sunset) — el sol bajando entre -6° y 6°.
        let base = 1_704_067_200; // 2024-01-01T00:00:00Z
        let found = (0..(24 * 60)).any(|min| {
            let p = light_phase(-34.6, -58.4, base + min * 60).unwrap_or_default();
            p.contains("sunset")
        });
        assert!(found, "debería existir algún instante de atardecer en el día");
    }
}
