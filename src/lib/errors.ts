// Localiza los mensajes de error del backend (Rust devuelve `Result<_, String>`
// en español) al idioma actual, sin tocar el backend ni sus tests. Se aplica en
// el punto donde se muestra el error (App.tsx). Si ningún patrón matchea, se
// devuelve el mensaje crudo tal cual (los errores raros del SO pasan de largo).
type TFn = (key: string, vars?: Record<string, string | number>) => string;

interface Rule {
  re: RegExp;
  key: string;
  // Mapea los grupos capturados a variables de interpolación.
  vars?: (m: RegExpMatchArray) => Record<string, string | number>;
}

// Primer match gana. El orden importa: las variantes más específicas primero.
const RULES: Rule[] = [
  { re: /^no hay catálogo abierto: creá o abrí uno antes de escanear$/, key: "error.noCatalogScan" },
  { re: /^no hay catálogo abierto$/, key: "error.noCatalog" },
  { re: /^abrí un catálogo antes de compartirlo$/, key: "error.openCatalogFirst" },
  { re: /^la ruta (.+) no existe o el disco no está montado$/, key: "error.pathMissing", vars: (m) => ({ path: m[1] }) },
  { re: /^No se encontró el original en (.+)\. ¿Es el disco correcto\?$/, key: "error.originalNotFound", vars: (m) => ({ path: m[1] }) },
  { re: /^El disco «(.+)» está offline\..*$/, key: "error.diskOffline", vars: (m) => ({ name: m[1] }) },
  { re: /^el volumen montado no coincide con el fingerprint del disco$/, key: "error.fingerprintMismatch" },
  { re: /^el original no está en el disco$/, key: "error.originalNotOnDisk" },
  { re: /^ruta fuera del volumen permitido$/, key: "error.pathOutsideVolume" },
  { re: /^sin acceso a ese disco$/, key: "error.noDiskAccess" },
  { re: /^el dispositivo no tiene acceso a ese disco$/, key: "error.deviceNoDiskAccess" },
  { re: /^no se pudo resolver el volumen$/, key: "error.volumeUnresolved" },
  { re: /^la entrada no existe$/, key: "error.entryMissing" },
  { re: /^no es un archivo$/, key: "error.notAFile" },
  { re: /^escaneo cancelado$/, key: "error.scanCancelled" },
  { re: /^error escaneando: (.+)$/, key: "error.scanError", vars: (m) => ({ e: m[1] }) },
  { re: /^error guardando el escaneo: (.+)$/, key: "error.saveScanError", vars: (m) => ({ e: m[1] }) },
  { re: /^error en ingesta: (.+)$/, key: "error.ingestError", vars: (m) => ({ e: m[1] }) },
  { re: /^error abriendo catálogo: (.+)$/, key: "error.openCatalogError", vars: (m) => ({ e: m[1] }) },
  { re: /^el archivo \.dcmf no contiene discos reconocibles$/, key: "error.dcmfNoDisks" },
  { re: /^no se pudo leer (.+?): (.+)$/, key: "error.readError", vars: (m) => ({ path: m[1], e: m[2] }) },
  { re: /^no se pudo escribir (.+?): (.+)$/, key: "error.writeError", vars: (m) => ({ path: m[1], e: m[2] }) },
  { re: /^no se pudo mover a la papelera: (.+)$/, key: "error.trashError", vars: (m) => ({ e: m[1] }) },
  { re: /^no se pudo abrir: (.+)$/, key: "error.openError", vars: (m) => ({ e: m[1] }) },
  { re: /^formato de archivo no soportado: (.+)$/, key: "error.unsupportedFormat", vars: (m) => ({ ext: m[1] }) },
  { re: /^no se pudo generar preview \(formato no soportado\): (.+)$/, key: "error.previewError", vars: (m) => ({ e: m[1] }) },
  { re: /^preview de RAW no disponible en esta plataforma$/, key: "error.rawPreviewUnavailable" },
  { re: /^el archivo no contiene un stream de video reconocible$/, key: "error.noVideoStream" },
  { re: /^ffmpeg no está disponible$/, key: "error.ffmpegMissing" },
  { re: /^ffprobe no está disponible$/, key: "error.ffprobeMissing" },
  { re: /^ffmpeg no pudo extraer el frame$/, key: "error.ffmpegFrame" },
  { re: /^ffprobe falló al leer el archivo$/, key: "error.ffprobeRead" },
  { re: /^(ZIP|RAR|7z) inválido: (.+)$/, key: "error.badArchive", vars: (m) => ({ kind: m[1], e: m[2] }) },
  // Conector remoto / dispositivos
  { re: /^el conector no está activo$/, key: "error.connectorOff" },
  { re: /^código de emparejamiento inválido$/, key: "error.badPairingCode" },
  { re: /^token de dispositivo inválido$/, key: "error.badDeviceToken" },
  { re: /^token inválido o expirado$/, key: "error.tokenExpired" },
];

export function localizeError(raw: string, t: TFn): string {
  for (const rule of RULES) {
    const m = raw.match(rule.re);
    if (m) return t(rule.key, rule.vars?.(m));
  }
  return raw;
}
