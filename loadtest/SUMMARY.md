# Comparative summary: Rust vs PHP (k6)

Sources: [`results/results-local.json`](results/results-local.json) (Rust) and [`results/results-php.json`](results/results-php.json) (PHP). Same Docker network (`image_proccesor_*:8080`), same `API_TOKEN` via `rust/.env`.

**Interactive charts:** open [`report.html`](report.html) in a browser (no server required).

## Run context

| | Rust | PHP |
|---|------|-----|
| `BASE_URL` (internal) | `http://image_proccesor_rust:8080` | `http://image_proccesor_php:8080` |
| `vus_max` | 50 | 50 |
| Total HTTP requests | 3820 | 128 |
| Request rate (`http_reqs` rate) | **22.27**/s | **0.64**/s |
| `http_req_failed` | **0%** | **~32.8%** |
| Global checks (`checks` value) | 100% | **~67%** |
| Resize OK (`resize status 200` passes) | 3513 | 77 |

The **PHP** run saturated at 50 VUs: many requests waited ~60s (`http_req_duration` median ~46s), failures, and few completed iterations. This is **not a fair throughput comparison** under the same load; it still shows order of magnitude and the PHP bottleneck (single-threaded built-in server per process).

To compare throughput fairly, rerun PHP with a **lower `STRESS_MAX_VUS`** (e.g. 6–10) and the same scenario length until `http_req_failed` ≈ 0%.

## Main table (k6 metrics + aggregated JSON body)

**avg** / **p(95)** come from the k6 summary (`Trend` on successful **200** resize responses with parseable JSON).

| Metric | Rust avg | Rust p(95) | PHP avg | PHP p(95) |
|--------|----------|------------|---------|-----------|
| `app_processing_time_ms` (ms) | 1173 | 1803 | 1902 | 2716 |
| `app_timing_download_ms` | 1128 | 1682 | 1639 | 2279 |
| `app_timing_resize_ms` | 38 | 116 | 80 | 292 |
| `app_timing_save_ms` | 7 | 21 | 173 | 583 |
| `app_memory_peak_estimate_kb` | 9577 | 28210 | 8191 | 19351 |
| `app_memory_source_decoded_kb` | 3890 | 7689 | 3795 | 6997 |
| `http_req_duration` (ms, network + queue + app) | 1080 | 1770 | 40577 | 59997 |

### Quick read

- **Server-side resize time** (`app_timing_resize_ms`): Rust ~**38 ms** avg vs PHP ~**80 ms** on successful samples (PHP has fewer samples and queueing).
- **`processing_time_ms`** (API field): averages **1173 ms** vs **1902 ms**; much of that is **remote image download** (`picsum`), not only local CPU.
- **Estimated memory peak** (`peak_estimate_kb`): similar averages (~8–9.5k in JSON “kb” units); Rust p95 is higher on large-image requests (~28k p95).

## Suggested next steps

1. Rerun PHP with **fewer VUs** until ~0% failures; export `results-php.json` again.
2. Refresh [`report.html`](report.html) if you paste new numbers (or automate with a JSON reader script).
3. Optional: same test against **Sprites** (public `BASE_URL`) and add a third column to the report.
