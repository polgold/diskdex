# DiskDex

### Encontrá cualquier archivo de tus discos de respaldo — sin enchufar un solo disco.

---

## El problema

Una productora audiovisual acumula años de proyectos en discos de respaldo. Hoy son
**~54 discos**, **6,8 millones de archivos**, más de **116 TB**. Cuando entra un trabajo
nuevo y hay que recuperar material viejo, la única opción es ir enchufando disco por disco
hasta dar con el correcto. Horas perdidas por cada búsqueda.

DiskCatalogMaker resolvía parte de esto, pero el catálogo histórico quedó atrapado en su
formato propietario `.dcmf` y la herramienta no trae los archivos de vuelta cuando el disco
está en otra máquina o hay que mandarlos a la nube.

---

## La solución

**DiskDex** es una app de escritorio (macOS y Windows) que indexa el contenido de todos
los discos y te deja **buscar y navegar aunque estén desconectados**. Importa el catálogo
histórico sin perder nada, escanea discos nuevos apenas los conectás, y está pensada para
sumar un conector seguro que traiga los archivos reales desde la red local a la nube o a otro
dispositivo — sin mover el disco.

> Búsqueda de `.mov` en el catálogo real: **261.991 resultados en medio segundo**.

---

## Lo que ya funciona

### 1. Importa tu catálogo histórico tal cual
Importa el `.dcmf` de DiskCatalogMaker sin pérdida: nombres, jerarquía completa, **tamaños
reales de archivos de hasta cientos de GB** y fechas. Probado contra el catálogo real:

- **54 discos** y **6.828.850 entradas** reconstruidas (cifra exacta).
- Rutas completas correctas — ej. `…/SF28/HUFNAGL PILAR/private/M4ROOT/CLIP/C0001.MP4`.
- El archivo más grande del catálogo: **174,94 GB**, leído sin truncar.

### 2. Escanea discos nuevos apenas los conectás
Conectá un disco y DiskDex lo detecta solo: aparece un aviso **"Disco detectado — Escanear
ahora"**. El escaneo guarda el árbol completo con tamaño lógico **y físico** (lo realmente
ocupado en disco) y fechas, más una **huella digital del volumen** para reconocerlo la próxima
vez. Si reconectás un disco ya catalogado, **re-escanear lo actualiza** sin duplicarlo.

### 3. Sabe qué disco está conectado
Cada disco aparece marcado como **online / offline** según lo que esté montado en ese momento,
con su capacidad y cantidad de archivos.

### 4. Búsqueda instantánea (motor listo)
Búsqueda full-text por nombre sobre **todos los discos a la vez**, con la ruta completa y el
disco de cada resultado. Sobre 6,8 millones de entradas responde en **menos de un segundo**.

---

## Cómo se ve la demo

1. **Abrir DiskDex** → modo oscuro, sobrio, tipo herramienta de post.
2. **Importar `.dcmf`** → en pocos minutos quedan los 54 discos en una grilla, cada uno con
   su tamaño y cantidad de archivos.
3. **Conectar un disco USB** → salta el banner ámbar *"Disco detectado: NAME (1.8 TB)"* →
   clic en **Escanear ahora** → queda incorporado al catálogo.
4. **Buscar** (motor) → escribir `.mov` o un nombre de clip → resultados con disco + ruta,
   al instante, sin importar si el disco está conectado.

---

## Cómo está hecho (en breve)

- **Tauri 2 + Rust** para el motor (importar, escanear, indexar): binario chico, rápido y
  multiplataforma. El trabajo pesado nunca traba la interfaz.
- **React + TypeScript** para una UI moderna y accesible (modo oscuro, atajos, virtualización).
- **SQLite + búsqueda full-text** como base: escala a millones de archivos en un solo archivo
  de catálogo portable.
- **Seguridad por diseño** para el futuro conector: solo lectura, autenticado por dispositivo,
  cifrado, y nunca expone nada fuera del catálogo.

---

## Roadmap

| Fase | Qué aporta | Estado |
|---|---|---|
| Importar `.dcmf` | Traer el catálogo histórico completo | ✅ Listo y validado |
| Escanear al conectar | Sumar discos nuevos sin fricción | ✅ Listo |
| Online/offline | Saber qué disco está a mano | ✅ Listo |
| Navegación 3 paneles | Explorar tipo Finder/Explorer | 🔜 Próximo (motor listo) |
| Búsqueda en pantalla | Buscar y filtrar desde la UI | 🔜 Próximo (motor listo) |
| Filtros avanzados | Por tipo, tamaño, fecha, combinaciones | ⏳ |
| Acciones | Revelar/abrir el original, copiar ruta | ⏳ |
| Exportar / reportes | CSV, JSON, PDF | ⏳ |
| Duplicados y estadísticas | Limpieza y visión del archivo | ⏳ |
| **Conector remoto seguro** | Traer archivos de la LAN a la nube o a otro equipo | ⏳ |

---

## En una línea

> **DiskDex convierte 116 TB repartidos en 54 discos en un buscador instantáneo —
> y, próximamente, en una forma segura de traer cualquier archivo sin tocar el disco.**

---

*Documento de presentación · estado verificado al cierre de la última sesión de desarrollo.
Detalle técnico en [`HANDOFF.md`](./HANDOFF.md).*
