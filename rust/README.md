# image_proccesor

API REST en Rust para descargar imágenes desde una URL, redimensionarlas y guardarlas en disco. Diseñada para alta concurrencia y bajo uso de RAM.

## Stack

- **Axum 0.7** + **Tokio** + **Hyper 1.x**: miles de conexiones concurrentes con muy poco overhead por request.
- **`fast_image_resize`** (SIMD acelerado) para el resize, mucho más rápido que `image::imageops`.
- **`image`** solo para decode/encode (JPEG, PNG, WebP).
- **`reqwest`** con cliente compartido (connection pooling, HTTP/2, `rustls`).
- Todo el trabajo CPU-bound corre dentro de `spawn_blocking` con un semáforo que limita los workers concurrentes a `num_cpus` por default — evita saturar la CPU y mantiene el reactor async libre.
- Auth por **Bearer token** con comparación constant-time (`subtle`).
- Imagen Docker multi-stage, usuario no-root.

## Estructura del proyecto

```
image_proccesor/
├── rust/
│   ├── Cargo.toml
│   ├── Dockerfile
│   ├── docker-compose.yml
│   ├── .env.example
│   └── src/ …
├── php/
│   ├── api.php
│   ├── Dockerfile
│   └── .env.example
├── loadtest/
├── docker-compose.yml   # Rust + PHP, env: rust/.env
└── .env.example
```

## Configuración (variables de entorno)

| Variable | Default | Descripción |
| --- | --- | --- |
| `API_TOKEN` | (requerida) | Token Bearer que valida el middleware de auth |
| `BIND_ADDR` | `0.0.0.0:8080` | Dirección de bind del server |
| `OUTPUT_DIR` | `/data/images` | Dónde se guardan las imágenes procesadas |
| `MAX_DOWNLOAD_BYTES` | `26214400` (25 MiB) | Tamaño máximo de la imagen origen descargada |
| `DOWNLOAD_TIMEOUT_SECS` | `15` | Timeout total de la descarga |
| `MAX_BODY_BYTES` | `1048576` (1 MiB) | Tamaño máximo del JSON del request |
| `RESIZE_WORKERS` | `num_cpus` | Workers de resize concurrentes (semáforo) |
| `JPEG_QUALITY` | `85` | Calidad JPEG (1–100) |
| `MAX_SCALE` | `10` | Factor de escala máximo aceptado |
| `RUST_LOG` | `info` | Filtro de logs (`tracing-subscriber`) |

Hay un `.env.example` con todos los valores listos para copiar. El **docker compose de la raíz del monorepo** usa el mismo archivo `rust/.env` para el servicio Rust y el PHP (véase [README.md](../README.md) en la raíz).

## Endpoints

### `GET /health`

Sin auth. Devuelve `200 {"status":"ok"}`.

### `POST /images/resize`

Requiere `Authorization: Bearer <API_TOKEN>` y `Content-Type: application/json`.

**Request body** (al menos uno de `width`, `height` o `scale` es requerido):

```json
{
  "url": "https://example.com/foo.jpg",
  "width": 800,
  "height": 600,
  "scale": 0.5,
  "format": "jpeg"
}
```

Reglas:

- Si vienen `width` y `height`: resize exacto (puede deformar).
- Si viene solo `width`: el `height` se calcula manteniendo aspect ratio.
- Si viene solo `height`: el `width` se calcula manteniendo aspect ratio.
- Si viene `scale`: tiene prioridad y multiplica las dimensiones originales.
- `format` opcional: `jpeg` | `png` | `webp`. Si se omite, se usa el formato detectado en la imagen origen (fallback a `png`).

**Response 200**:

```json
{
  "path": "/data/images/3f2a....jpg",
  "filename": "3f2a....jpg",
  "width": 800,
  "height": 600,
  "size_kb": 120.56,
  "format": "jpg",
  "content_type": "image/jpeg",
  "processing_time_ms": 87,
  "timing": {
    "download_ms": 41,
    "resize_ms": 38,
    "save_ms": 8,
    "total_ms": 87
  },
  "memory_kb": {
    "source_compressed_kb": 240.0,
    "source_decoded_kb": 11718.75,
    "output_decoded_kb": 1875.0,
    "output_encoded_kb": 120.56,
    "peak_estimate_kb": 13954.31
  }
}
```

Todos los tamaños están en **KiB (1 KB = 1024 bytes)**, redondeados a 2 decimales. Sobre `memory_kb`: en Rust no se puede medir RAM "por request" en aislamiento (todas las requests comparten el allocator global), así que reportamos el tamaño de los buffers principales que la request mantuvo en memoria, que es lo que domina el consumo real:

- `source_compressed_kb`: tamaño descargado (imagen comprimida).
- `source_decoded_kb`: imagen origen decodificada en RAM (`src_w × src_h × 4 / 1024`).
- `output_decoded_kb`: imagen redimensionada decodificada (`dst_w × dst_h × 4 / 1024`).
- `output_encoded_kb`: tamaño del archivo final escrito a disco.
- `peak_estimate_kb`: suma de los anteriores — cota superior aproximada del pico de RAM usado por esa request.

**Errores**:

| Status | Causa |
| --- | --- |
| 400 | Body inválido, dimensiones inválidas, falta width/height/scale |
| 401 | Falta el header `Authorization` o token incorrecto |
| 413 | Imagen remota supera `MAX_DOWNLOAD_BYTES` o body excede `MAX_BODY_BYTES` |
| 415 | El formato de la imagen origen no es decodificable |
| 422 | No se pudo descargar la URL (timeout, status != 2xx, DNS) |
| 500 | Error interno (encode, IO, etc.) |

## Levantar el proyecto con Docker

### Monorepo (Rust + PHP, mismo `rust/.env`)

Desde la **raíz** del repositorio (recomendado):

```bash
cp .env.example rust/.env
# editar rust/.env y poner un API_TOKEN serio

docker compose up -d --build
```

- Rust: `http://localhost:8080/health`
- PHP: `http://localhost:8081/health`

Las imágenes Rust se guardan en `./data/rust`; las PHP en `./data/php`.

### Solo Rust (desde esta carpeta `rust/`)

```bash
cp .env.example .env
docker compose up -d --build
```

Persistencia en `./data` relativa a `rust/`.

## Ejemplos de uso

Resize manteniendo aspect ratio (solo ancho):

```bash
curl -X POST http://localhost:8080/images/resize \
  -H "Authorization: Bearer cambia-este-token-en-produccion" \
  -H "Content-Type: application/json" \
  -d '{"url":"https://picsum.photos/2000","width":400,"format":"webp"}'
```

Resize a tamaño exacto:

```bash
curl -X POST http://localhost:8080/images/resize \
  -H "Authorization: Bearer cambia-este-token-en-produccion" \
  -H "Content-Type: application/json" \
  -d '{"url":"https://picsum.photos/2000","width":640,"height":480,"format":"jpeg"}'
```

Resize por factor de escala:

```bash
curl -X POST http://localhost:8080/images/resize \
  -H "Authorization: Bearer cambia-este-token-en-produccion" \
  -H "Content-Type: application/json" \
  -d '{"url":"https://picsum.photos/2000","scale":0.5}'
```

## Build local sin Docker

Requiere Rust 1.75+:

```bash
export API_TOKEN=mi-token
cargo run --release
```

## Notas de performance

- En carga sostenida el bottleneck real es la CPU del resize. El semáforo evita que en un pico de requests se disparen miles de tasks blocking que terminarían thrasheando la pool de Tokio.
- `reqwest` mantiene conexiones HTTP/2 reutilizadas por host, evitando re-handshake TLS.
- El profile `release` está configurado con `lto = "fat"`, `codegen-units = 1`, `strip`, `panic = "abort"` para reducir el binario y mejorar throughput.
- Si el origen sirve imágenes muy grandes, ajustar `MAX_DOWNLOAD_BYTES` con criterio: en RAM se mantiene la imagen comprimida + la decodificada (RGBA8 = `width * height * 4` bytes).
