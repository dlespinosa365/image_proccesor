# API PHP (réplica para benchmarks)

Mismo contrato que el servicio Rust en `../rust/`. Variables: ver `../rust/.env.example` o `../.env.example` (deben coincidir con `rust/.env` usado por el compose de la raíz).

## Docker (recomendado)

Desde la **raíz** del repo:

```bash
docker compose up -d --build image_proccesor_php
```

Puerto en el host: **8081** → `http://127.0.0.1:8081/health`.

## Local (sin Docker)

Requiere PHP 8.1+ con `curl`, `gd`, `json`. Copiá las mismas variables que en Rust (`API_TOKEN`, `OUTPUT_DIR`, etc.) y:

```bash
php -S 0.0.0.0:8081 api.php
```
