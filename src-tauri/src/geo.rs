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
}
