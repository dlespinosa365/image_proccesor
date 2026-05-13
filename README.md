# image_proccesor (monorepo)

Image resize API with the same contract in two implementations:

| Path | Role |
|------|------|
| [rust/](rust/) | Primary service (Rust) |
| [php/](php/) | Reference / benchmark implementation (PHP, built-in server) |
| [loadtest/](loadtest/) | Load tests with **k6** |

Default ports with root **Docker Compose**: **Rust `8080`**, **PHP `8081`**. Processed files are written under `./data/rust` and `./data/php` (ignored by git).

---

## Requirements

- **Docker** and **Docker Compose** v2
- For load tests: the `grafana/k6` image (recommended on Windows) or [k6 on the host](https://k6.io/docs/get-started/installation/)

---

## 1. Configuration (`rust/.env`)

Compose uses a **single** env file for both Rust and PHP:

```bash
# From the repository root
cp .env.example rust/.env
```

Edit `rust/.env` and set at least **`API_TOKEN`** (k6 must use the same value). Other keys match [rust/.env.example](rust/.env.example) and [php/.env.example](php/.env.example).

On Windows, if `.env` is saved with CRLF line endings, the Rust container trims the token; if you change env-related code, rebuild: `docker compose build --no-cache`.

---

## 2. Start containers

**Rust and PHP together** (from the repo root):

```bash
docker compose up -d --build
```

**One service only:**

```bash
docker compose up -d --build image_proccesor_rust
docker compose up -d --build image_proccesor_php
```

Health checks:

| Service | URL |
|---------|-----|
| Rust | <http://127.0.0.1:8080/health> |
| PHP | <http://127.0.0.1:8081/health> |

Stop:

```bash
docker compose down
```

Compose file: [docker-compose.yml](docker-compose.yml).

---

## 3. Load tests (k6)

Script: [loadtest/k6/resize.js](loadtest/k6/resize.js). More detail: [loadtest/k6/README.md](loadtest/k6/README.md).

### Main environment variables

| Variable | Purpose |
|----------|---------|
| `BASE_URL` | API base URL **without** a trailing slash |
| `API_TOKEN` | Same as `API_TOKEN` in `rust/.env` |
| `STRESS_MAX_VUS` | Peak virtual users (default `25`, max `200`) |
| `STRESS_QUICK=1` | Short run (~60 s) instead of the long ramp |
| `HEALTH_RATIO` | Fraction of iterations that only call `/health` (default `0.08`) |

PHP with `php -S` is a **single process**; many VUs saturate quickly. For stable app metrics and few failures, use a **low `STRESS_MAX_VUS`** (e.g. 6–10) until `http_req_failed` ≈ 0.

### Windows + Docker Desktop (recommended)

On many setups, requests from the **host** to `localhost:808x` **drop the `Authorization` header**, so the API returns **401**. Run k6 **inside Docker** on the same network as Compose.

The network name is usually `{repo_folder}_default`. If the folder is `image_proccesor`, use **`image_proccesor_default`**. If your folder name differs, list networks and pick the compose project network, for example:

```powershell
docker network ls | Select-String image_proccesor
```

```bash
docker network ls | grep image_proccesor
```

Create the results directory if needed:

```powershell
New-Item -ItemType Directory -Force -Path "loadtest\results" | Out-Null
```

**Rust** (`image_proccesor_rust`, internal port **8080**):

```powershell
docker run --rm `
  --network image_proccesor_default `
  --env-file "rust\.env" `
  -e "BASE_URL=http://image_proccesor_rust:8080" `
  -e "STRESS_QUICK=1" `
  -e "STRESS_MAX_VUS=10" `
  -v "${PWD}/loadtest:/t" `
  -w /t/k6 `
  grafana/k6 run resize.js --summary-export=/t/results/results-local.json
```

**PHP** (`image_proccesor_php`):

```powershell
docker run --rm `
  --network image_proccesor_default `
  --env-file "rust\.env" `
  -e "BASE_URL=http://image_proccesor_php:8080" `
  -e "STRESS_QUICK=1" `
  -e "STRESS_MAX_VUS=10" `
  -v "${PWD}/loadtest:/t" `
  -w /t/k6 `
  grafana/k6 run resize.js --summary-export=/t/results/results-php.json
```

Remove `STRESS_QUICK=1` for the full staged ramp. Raise `STRESS_MAX_VUS` only if k6 thresholds pass and the service stays healthy.

**Same approach with bash** (adjust `--network` if your project name differs):

```bash
mkdir -p loadtest/results
docker run --rm \
  --network image_proccesor_default \
  --env-file rust/.env \
  -e BASE_URL=http://image_proccesor_rust:8080 \
  -e STRESS_QUICK=1 \
  -e STRESS_MAX_VUS=10 \
  -v "$(pwd)/loadtest:/t" \
  -w /t/k6 \
  grafana/k6 run resize.js --summary-export=/t/results/results-local.json
```

### k6 on the host (Linux / macOS, or when `Authorization` works)

**PowerShell** (from `loadtest/k6`):

```powershell
$env:BASE_URL = "http://127.0.0.1:8080"
$env:API_TOKEN = "<same API_TOKEN as in rust/.env>"
$env:STRESS_MAX_VUS = "15"
k6 run resize.js --summary-export=../results/results-local.json
```

**bash:**

```bash
cd loadtest/k6
export BASE_URL=http://127.0.0.1:8080
export API_TOKEN='<same API_TOKEN as in rust/.env>'
export STRESS_MAX_VUS=15
k6 run resize.js --summary-export=../results/results-local.json
```

Exported JSON files go under `loadtest/results/` (gitignored unless you choose to commit them).

---

## 4. Report and summary

- Tables and narrative: [loadtest/SUMMARY.md](loadtest/SUMMARY.md)
- Static charts (paste numbers from JSON into `const R` / `const P`): [loadtest/report.html](loadtest/report.html)
- Disk cleanup after heavy runs: [loadtest/README.md](loadtest/README.md)

---

## 5. Quick links

- Rust API and env: [rust/README.md](rust/README.md)
- PHP local / Docker: [php/README.md](php/README.md)
- Compose from `rust/` only: [rust/docker-compose.yml](rust/docker-compose.yml)
