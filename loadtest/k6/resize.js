/**
 * k6 load test: GET /health (mix ~8%) + POST /images/resize (Bearer).
 *
 * Required env:
 *   BASE_URL   e.g. http://127.0.0.1:8080 or https://your-sprite.sprites.app
 *   API_TOKEN  same as server API_TOKEN
 *
 * Optional:
 *   STRESS_MAX_VUS  peak virtual users (default 25, cap 200)
 *   HEALTH_RATIO    0–1 fraction of iterations that only hit /health (default 0.08)
 *   STRESS_QUICK=1  short run (fixed duration) instead of ramping stages
 *
 * Métricas desde el JSON de resize (Trend → avg / p95 en el resumen y en --summary-export):
 *   app_processing_time_ms, app_timing_*_ms, app_memory_*_kb
 *
 * Windows + Docker Desktop: published localhost often strips `Authorization`.
 * Run k6 on the same Docker network as the API, e.g.:
 *   docker run --rm --network image_proccesor_default --env-file rust/.env
 *     -e BASE_URL=http://image_proccesor_rust:8080 -e STRESS_QUICK=1
 *     -v "%CD%/loadtest:/t" -w /t/k6 grafana/k6 run resize.js --summary-export=/t/results/results-local.json
 */
import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Trend } from 'k6/metrics';

const processingTime = new Trend('app_processing_time_ms', true);
const timingDownloadMs = new Trend('app_timing_download_ms', true);
const timingResizeMs = new Trend('app_timing_resize_ms', true);
const timingSaveMs = new Trend('app_timing_save_ms', true);
const timingTotalMs = new Trend('app_timing_total_ms', true);
const memSourceCompressedKb = new Trend('app_memory_source_compressed_kb', true);
const memSourceDecodedKb = new Trend('app_memory_source_decoded_kb', true);
const memOutputDecodedKb = new Trend('app_memory_output_decoded_kb', true);
const memOutputEncodedKb = new Trend('app_memory_output_encoded_kb', true);
const memPeakEstimateKb = new Trend('app_memory_peak_estimate_kb', true);
const resizeErrors = new Counter('resize_errors');

function addTrendNumber(trend, value) {
  if (typeof value === 'number' && Number.isFinite(value)) {
    trend.add(value);
  }
}

function env(name, def) {
  const v = __ENV[name];
  if (v === undefined || v === '') return def;
  return v;
}

function intEnv(name, def, min, max) {
  const n = parseInt(env(name, String(def)), 10);
  if (Number.isNaN(n)) return def;
  return Math.min(max, Math.max(min, n));
}

function floatEnv(name, def) {
  const n = parseFloat(env(name, String(def)));
  if (Number.isNaN(n)) return def;
  return n;
}

function randomInt(min, max) {
  return Math.floor(Math.random() * (max - min + 1)) + min;
}

function stripTrailingSlash(s) {
  return s.replace(/\/+$/, '');
}

function buildResizePayload() {
  const seed = randomInt(1, 5000);
  const sw = randomInt(400, 1600);
  const sh = randomInt(400, 1600);
  const url = `https://picsum.photos/seed/k6-${seed}/${sw}/${sh}`;
  const mode = randomInt(0, 3);
  /** @type {Record<string, unknown>} */
  const payload = { url };

  if (mode === 0) {
    payload.width = randomInt(200, Math.min(2000, sw + 400));
  } else if (mode === 1) {
    payload.height = randomInt(200, Math.min(2000, sh + 400));
  } else if (mode === 2) {
    payload.width = randomInt(200, 1200);
    payload.height = randomInt(200, 1200);
  } else {
    const s = Math.round((0.12 + Math.random() * 2.88) * 100) / 100;
    payload.scale = s;
  }

  const fmtRoll = Math.random();
  if (fmtRoll < 0.22) payload.format = 'jpeg';
  else if (fmtRoll < 0.44) payload.format = 'png';
  else if (fmtRoll < 0.66) payload.format = 'webp';

  return payload;
}

const maxVus = intEnv('STRESS_MAX_VUS', 25, 1, 200);
const quick = env('STRESS_QUICK', '') === '1';

const fullOptions = {
  setupTimeout: '60s',
  stages: [
    { duration: '20s', target: Math.max(1, Math.ceil(maxVus * 0.2)) },
    { duration: '40s', target: Math.max(1, Math.ceil(maxVus * 0.5)) },
    { duration: '60s', target: maxVus },
    { duration: '30s', target: maxVus },
    { duration: '20s', target: 0 },
  ],
  thresholds: {
    http_req_failed: ['rate<0.10'],
    http_req_duration: ['p(95)<120000'],
    checks: ['rate>0.85'],
  },
};

const quickOptions = {
  setupTimeout: '60s',
  vus: Math.max(1, Math.min(maxVus, 10)),
  duration: '60s',
  thresholds: {
    http_req_failed: ['rate<0.10'],
    http_req_duration: ['p(95)<120000'],
    checks: ['rate>0.85'],
  },
};

export const options = quick ? quickOptions : fullOptions;

export function setup() {
  const base = env('BASE_URL', '');
  const token = env('API_TOKEN', '');
  if (!base) {
    throw new Error('BASE_URL is required (e.g. -e BASE_URL=http://127.0.0.1:8080)');
  }
  if (!token) {
    throw new Error('API_TOKEN is required (same value as the server env API_TOKEN)');
  }
  return {
    base: stripTrailingSlash(base),
    token,
    healthRatio: floatEnv('HEALTH_RATIO', 0.08),
  };
}

export default function (data) {
  const { base, token, healthRatio } = data;

  if (Math.random() < healthRatio) {
    const res = http.get(`${base}/health`, {
      tags: { name: 'Health' },
    });
    check(res, {
      'health status 200': (r) => r.status === 200,
      'health body ok': (r) => String(r.body).includes('"ok"'),
    });
    sleep(randomInt(0, 2) / 10);
    return;
  }

  const body = buildResizePayload();
  const res = http.post(`${base}/images/resize`, JSON.stringify(body), {
    headers: {
      'Content-Type': 'application/json',
      Authorization: `Bearer ${token}`,
    },
    tags: { name: 'Resize' },
  });

  const ok = check(res, {
    'resize status 200': (r) => r.status === 200,
    'resize json width': (r) => {
      if (r.status !== 200) return false;
      try {
        const j = JSON.parse(String(r.body));
        return typeof j.width === 'number' && j.width > 0;
      } catch {
        return false;
      }
    },
  });

  if (!ok) {
    resizeErrors.add(1);
  } else {
    try {
      const j = JSON.parse(String(res.body));
      addTrendNumber(processingTime, j.processing_time_ms);
      const t = j.timing;
      if (t && typeof t === 'object') {
        addTrendNumber(timingDownloadMs, t.download_ms);
        addTrendNumber(timingResizeMs, t.resize_ms);
        addTrendNumber(timingSaveMs, t.save_ms);
        addTrendNumber(timingTotalMs, t.total_ms);
      }
      const m = j.memory_kb;
      if (m && typeof m === 'object') {
        addTrendNumber(memSourceCompressedKb, m.source_compressed_kb);
        addTrendNumber(memSourceDecodedKb, m.source_decoded_kb);
        addTrendNumber(memOutputDecodedKb, m.output_decoded_kb);
        addTrendNumber(memOutputEncodedKb, m.output_encoded_kb);
        addTrendNumber(memPeakEstimateKb, m.peak_estimate_kb);
      }
    } catch {
      /* ignore */
    }
  }

  sleep(randomInt(1, 4) / 10);
}
