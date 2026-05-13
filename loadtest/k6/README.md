# k6 load tests

Pruebas de concurrencia contra `GET /health` y `POST /images/resize` con JSON aleatorio (URL picsum con seed fija por request, `width` / `height` / `scale`, `format` opcional).

## Requisitos

- [k6](https://k6.io/docs/get-started/installation/) instalado (`k6 version`).
- El servidor en marcha: Rust y/o PHP vía [docker compose](../../docker-compose.yml) (mismo `API_TOKEN` en `rust/.env`) o tu propio despliegue.
- Red saliente desde el proceso del servidor hacia `https://picsum.photos` (descarga de origen).

## Variables de entorno

| Variable | Obligatoria | Descripción |
|----------|-------------|-------------|
| `BASE_URL` | Sí | Sin barra final. Rust en Docker (raíz): `http://127.0.0.1:8080`. PHP: `http://127.0.0.1:8081`. Sprites: `https://tu-sprite.sprites.app` |
| `API_TOKEN` | Sí | **Mismo** `API_TOKEN` que en `rust/.env` (compose inyecta el mismo archivo a Rust y PHP) |
| `STRESS_MAX_VUS` | No | Pico de usuarios virtuales (default `25`, máx. `200`) |
| `HEALTH_RATIO` | No | Fracción de iteraciones que solo llaman `/health` (default `0.08`) |

No commitees tokens. En PowerShell usá `$env:API_TOKEN = '...'` en la sesión actual.

## Valores del JSON de resize (RAM y tiempos de la app)

En cada `200` de `/images/resize`, el script lee el cuerpo y alimenta métricas **Trend** de k6 (solo resize exitoso; `/health` no aporta).

| Métrica k6 | Origen en el JSON |
|------------|-------------------|
| `app_processing_time_ms` | `processing_time_ms` |
| `app_timing_download_ms` | `timing.download_ms` |
| `app_timing_resize_ms` | `timing.resize_ms` |
| `app_timing_save_ms` | `timing.save_ms` |
| `app_timing_total_ms` | `timing.total_ms` |
| `app_memory_source_compressed_kb` | `memory_kb.source_compressed_kb` |
| `app_memory_source_decoded_kb` | `memory_kb.source_decoded_kb` |
| `app_memory_output_decoded_kb` | `memory_kb.output_decoded_kb` |
| `app_memory_output_encoded_kb` | `memory_kb.output_encoded_kb` |
| `app_memory_peak_estimate_kb` | `memory_kb.peak_estimate_kb` |

Al terminar el test, en consola y en `--summary-export` aparecen **avg**, **min**, **med**, **max**, **p(90)**, **p(95)**, etc. Eso es el “promedio” y la dispersión sobre todas las muestras capturadas (no solo la media aritmética: mirá **avg** y **p95**).

## Windows y Docker Desktop

En muchos equipos, las peticiones desde el **host** a `http://127.0.0.1:8080` **no llevan** el header `Authorization` hasta el contenedor (el middleware responde `401`). Para pruebas fiables:

1. Ejecutá k6 **dentro de Docker** en la misma red que el compose (`image_proccesor_default`), con `BASE_URL=http://image_proccesor_rust:8080` o `http://image_proccesor_php:8080` (puerto **interno** 8080).
2. O usá la URL pública de Sprites / un túnel donde el header no se pierda.

Ejemplo (PowerShell, desde la **raíz** del repo, con `docker compose` ya levantado):

```powershell
docker run --rm `
  --network image_proccesor_default `
  --env-file "rust\.env" `
  -e "BASE_URL=http://image_proccesor_rust:8080" `
  -e "STRESS_QUICK=1" `
  -e "STRESS_MAX_VUS=6" `
  -v "${PWD}/loadtest:/t" `
  -w /t/k6 `
  grafana/k6 run resize.js --summary-export=/t/results/results-local.json
```

Para PHP (mismo `rust\.env`, mismo token):

```powershell
docker run --rm `
  --network image_proccesor_default `
  --env-file "rust\.env" `
  -e "BASE_URL=http://image_proccesor_php:8080" `
  -e "STRESS_QUICK=1" `
  -e "STRESS_MAX_VUS=6" `
  -v "${PWD}/loadtest:/t" `
  -w /t/k6 `
  grafana/k6 run resize.js --summary-export=/t/results/results-php.json
```

`STRESS_QUICK=1` hace una corrida corta (~60 s) en lugar de la rampa larga.

## Ejemplos (k6 en el host, Linux/macOS o si Authorization funciona)

Desde esta carpeta (`loadtest/k6`):

```powershell
# Mismo API_TOKEN que en rust/.env (usado por docker compose en la raíz)
$env:BASE_URL = "http://127.0.0.1:8080"   # Rust
# $env:BASE_URL = "http://127.0.0.1:8081"  # PHP
$env:API_TOKEN = "<mismo valor que API_TOKEN en rust/.env>"
$env:STRESS_MAX_VUS = "15"
k6 run resize.js --summary-export=../results/results-local.json
```

Sprites (rampa conservadora; subí `STRESS_MAX_VUS` solo si la red y el plan lo permiten):

```powershell
$env:BASE_URL = "https://image-proccesor-bspfr.sprites.app"
$env:API_TOKEN = "<mismo-token-que-en-sprite>"
$env:STRESS_MAX_VUS = "10"
k6 run resize.js --summary-export=../results/results-sprites.json
```

Si `k6` no está en el PATH tras instalar por winget, usá la ruta completa, por ejemplo:

`& "C:\Program Files\k6\k6.exe" run resize.js ...`

## Umbrales (thresholds)

Por defecto el script falla el exit code de k6 si:

- `http_req_failed` ≥ 10 %
- `p(95)` de duración HTTP ≥ 120 s
- checks < 85 %

Ajustá los `thresholds` en `resize.js` si necesitás límites más laxos o estrictos.

## Disco (`OUTPUT_DIR`)

Cada `resize` escribe un archivo nuevo. Entre corridas largas vaciá el directorio de salida del servidor (local o Sprite) para evitar llenar disco. Ver sección “Limpieza de salida” en [../README.md](../README.md).
