<?php
/**
 * Réplica mínima de la API Rust para pruebas de rendimiento.
 *
 * Requiere: PHP 8.1+, extensiones curl, gd, json.
 *
 * Ejemplo local:
 *   export API_TOKEN=secreto
 *   export OUTPUT_DIR=/tmp/php-images
 *   php -S 0.0.0.0:8081 api.php
 * Docker (compose en la raíz del repo): mismas variables que Rust en `rust/.env`; PHP publicado en el host en el puerto 8081.
 *
 * Rutas: GET /health | POST /images/resize (Authorization: Bearer …)
 */
declare(strict_types=1);

const MAX_OUTPUT_DIMENSION = 16384;

main();

function main(): void
{
    $path = parse_url($_SERVER['REQUEST_URI'] ?? '/', PHP_URL_PATH) ?: '/';
    if ($path === '') {
        $path = '/';
    }

    if ($path === '/health' && ($_SERVER['REQUEST_METHOD'] ?? '') === 'GET') {
        send_json(200, ['status' => 'ok']);
        return;
    }

    if ($path === '/images/resize' && ($_SERVER['REQUEST_METHOD'] ?? '') === 'POST') {
        handle_resize();
        return;
    }

    if ($path === '/images/resize') {
        header('Allow: POST', true, 405);
        echo 'Method Not Allowed';
        return;
    }

    header('Content-Type: text/plain; charset=utf-8', true, 404);
    echo 'Not Found';
}

function env_str(string $key, string $default): string
{
    $v = getenv($key);
    if ($v === false || trim($v) === '') {
        return $default;
    }
    return trim($v);
}

function env_int(string $key, int $default): int
{
    $v = getenv($key);
    if ($v === false || trim($v) === '') {
        return $default;
    }
    return max(0, (int) trim($v));
}

function env_float(string $key, float $default): float
{
    $v = getenv($key);
    if ($v === false || trim($v) === '') {
        return $default;
    }
    return (float) trim($v);
}

function require_api_token(): string
{
    $t = getenv('API_TOKEN');
    if ($t === false || trim($t) === '') {
        send_error(500, 'internal_error', 'internal error: API_TOKEN env var is required and must be non-empty');
        exit;
    }
    return trim($t);
}

function send_json(int $status, array $data): void
{
    header('Content-Type: application/json; charset=utf-8', true, $status);
    echo json_encode($data, JSON_UNESCAPED_SLASHES | JSON_UNESCAPED_UNICODE | JSON_THROW_ON_ERROR);
}

function send_error(int $status, string $code, string $message): void
{
    send_json($status, [
        'error' => [
            'code' => $code,
            'message' => $message,
        ],
    ]);
}

function verify_bearer(string $expectedToken): void
{
    $auth = $_SERVER['HTTP_AUTHORIZATION'] ?? $_SERVER['REDIRECT_HTTP_AUTHORIZATION'] ?? null;
    if ($auth === null || !is_string($auth)) {
        send_error(401, 'unauthorized', 'unauthorized');
        exit;
    }
    $auth = trim($auth);
    $token = null;
    if (str_starts_with($auth, 'Bearer ')) {
        $token = trim(substr($auth, 7));
    } elseif (str_starts_with($auth, 'bearer ')) {
        $token = trim(substr($auth, 7));
    }
    if ($token === null || $token === '') {
        send_error(401, 'unauthorized', 'unauthorized');
        exit;
    }
    $e = $expectedToken;
    if (strlen($e) !== strlen($token) || !hash_equals($e, $token)) {
        send_error(401, 'unauthorized', 'unauthorized');
        exit;
    }
}

function bytes_to_kb(int|float $bytes): float
{
    return round(((float) $bytes) / 1024.0 * 100.0) / 100.0;
}

function now_ms(): int
{
    return (int) round(microtime(true) * 1000.0);
}

function uuid_v4(): string
{
    $b = random_bytes(16);
    $b[6] = chr((ord($b[6]) & 0x0f) | 0x40);
    $b[8] = chr((ord($b[8]) & 0x3f) | 0x80);
    $h = bin2hex($b);
    return substr($h, 0, 8) . '-' . substr($h, 8, 4) . '-' . substr($h, 12, 4) . '-' . substr($h, 16, 4) . '-' . substr($h, 20, 12);
}

/**
 * @return array{0:string,1:?string} [body, content_type]
 */
function fetch_url(string $url, int $maxBytes, int $timeoutSecs): array
{
    $p = parse_url($url);
    if ($p === false || !isset($p['scheme'])) {
        send_error(400, 'bad_request', 'invalid request: invalid url: malformed URL');
        exit;
    }
    $host = $p['host'] ?? '';
    if (!is_string($host) || $host === '') {
        send_error(400, 'bad_request', 'invalid request: invalid url: missing host');
        exit;
    }
    $scheme = strtolower((string) $p['scheme']);
    if ($scheme !== 'http' && $scheme !== 'https') {
        send_error(400, 'bad_request', 'invalid request: unsupported url scheme: ' . $scheme);
        exit;
    }

    $ch = curl_init($url);
    if ($ch === false) {
        send_error(422, 'upstream_error', 'could not fetch source image: request failed: curl_init failed');
        exit;
    }

    curl_setopt_array($ch, [
        CURLOPT_RETURNTRANSFER => true,
        CURLOPT_FOLLOWLOCATION => true,
        CURLOPT_MAXREDIRS => 5,
        CURLOPT_PROTOCOLS => CURLPROTO_HTTP | CURLPROTO_HTTPS,
        CURLOPT_REDIR_PROTOCOLS => CURLPROTO_HTTP | CURLPROTO_HTTPS,
        CURLOPT_TIMEOUT => $timeoutSecs,
        CURLOPT_MAXFILESIZE => $maxBytes,
        CURLOPT_USERAGENT => 'image_proccesor-php/0.1',
        CURLOPT_HEADER => false,
    ]);

    $body = curl_exec($ch);
    $errno = curl_errno($ch);
    $http = (int) curl_getinfo($ch, CURLINFO_HTTP_CODE);
    curl_close($ch);

    if ($body === false) {
        if ($errno === CURLE_FILESIZE_EXCEEDED) {
            send_error(413, 'payload_too_large', 'payload too large: remote image exceeds max size of ' . $maxBytes . ' bytes');
            exit;
        }
        send_error(422, 'upstream_error', 'could not fetch source image: request failed: ' . curl_strerror($errno));
        exit;
    }

    if ($http < 200 || $http >= 300) {
        send_error(422, 'upstream_error', 'could not fetch source image: upstream returned status ' . $http);
        exit;
    }

    if (strlen($body) > $maxBytes) {
        send_error(413, 'payload_too_large', 'payload too large: remote image exceeds max size of ' . $maxBytes . ' bytes');
        exit;
    }

    return [$body, null];
}

/**
 * @return array{ext:string,ctype:string}
 */
function parse_output_format(string $raw): array
{
    $s = strtolower(trim($raw));
    return match ($s) {
        'jpeg', 'jpg' => ['ext' => 'jpg', 'ctype' => 'image/jpeg'],
        'png' => ['ext' => 'png', 'ctype' => 'image/png'],
        'webp' => ['ext' => 'webp', 'ctype' => 'image/webp'],
        default => throw new InvalidArgumentException('invalid request: unsupported output format: ' . $raw),
    };
}

function write_image(GdImage $im, string $path, string $ext, int $jpegQuality): void
{
    match ($ext) {
        'jpg' => imagejpeg($im, $path, $jpegQuality),
        'png' => imagepng($im, $path),
        'webp' => defined('IMG_WEBP_LOSSLESS')
            ? imagewebp($im, $path, IMG_WEBP_LOSSLESS)
            : imagewebp($im, $path, 100),
        default => throw new RuntimeException('unsupported ext: ' . $ext),
    };
}

/**
 * @return array{0:int,1:int}
 */
function calculate_target(
    int $srcW,
    int $srcH,
    ?int $width,
    ?int $height,
    ?float $scale,
    float $maxScale
): array {
    if ($scale !== null) {
        if (!is_finite($scale) || $scale <= 0.0) {
            send_error(400, 'bad_request', 'invalid request: scale must be a positive finite number');
            exit;
        }
        if ($scale > $maxScale) {
            send_error(400, 'bad_request', 'invalid request: scale exceeds the maximum allowed value of ' . $maxScale);
            exit;
        }
        $w = max(1, (int) round($srcW * $scale));
        $h = max(1, (int) round($srcH * $scale));
        return validate_dimensions($w, $h);
    }

    if ($width !== null && $height !== null) {
        return validate_dimensions($width, $height);
    }
    if ($width !== null && $height === null) {
        if ($width === 0) {
            send_error(400, 'bad_request', 'invalid request: width must be greater than zero');
            exit;
        }
        $ratio = $width / $srcW;
        $h = max(1, (int) round($srcH * $ratio));
        return validate_dimensions($width, $h);
    }
    if ($width === null && $height !== null) {
        if ($height === 0) {
            send_error(400, 'bad_request', 'invalid request: height must be greater than zero');
            exit;
        }
        $ratio = $height / $srcH;
        $w = max(1, (int) round($srcW * $ratio));
        return validate_dimensions($w, $height);
    }

    send_error(400, 'bad_request', 'invalid request: at least one of width, height, or scale is required');
    exit;
}

/**
 * @return array{0:int,1:int}
 */
function validate_dimensions(int $w, int $h): array
{
    if ($w === 0 || $h === 0) {
        send_error(400, 'bad_request', 'invalid request: target width and height must be greater than zero');
        exit;
    }
    if ($w > MAX_OUTPUT_DIMENSION || $h > MAX_OUTPUT_DIMENSION) {
        send_error(400, 'bad_request', 'invalid request: target dimensions exceed the maximum of ' . MAX_OUTPUT_DIMENSION . 'px');
        exit;
    }
    return [$w, $h];
}

/**
 * @return array{ext:string,ctype:string}
 */
function detect_output_format(string $bytes): array
{
    $info = @getimagesizefromstring($bytes);
    if ($info === false) {
        return ['ext' => 'png', 'ctype' => 'image/png'];
    }
    $t = $info[2] ?? 0;
    return match ($t) {
        IMAGETYPE_JPEG => ['ext' => 'jpg', 'ctype' => 'image/jpeg'],
        IMAGETYPE_PNG => ['ext' => 'png', 'ctype' => 'image/png'],
        IMAGETYPE_WEBP => ['ext' => 'webp', 'ctype' => 'image/webp'],
        default => ['ext' => 'png', 'ctype' => 'image/png'],
    };
}

function handle_resize(): void
{
    $expected = require_api_token();
    verify_bearer($expected);

    $ct = $_SERVER['CONTENT_TYPE'] ?? '';
    if (!is_string($ct) || stripos($ct, 'application/json') !== 0) {
        send_error(415, 'unsupported_media_type', 'unsupported media type: Expected request with `Content-Type: application/json`');
        exit;
    }

    $maxBody = env_int('MAX_BODY_BYTES', 1048576);
    $raw = file_get_contents('php://input', false, null, 0, $maxBody + 1);
    if ($raw === false) {
        send_error(400, 'bad_request', 'invalid request: could not read body');
        exit;
    }
    if (strlen($raw) > $maxBody) {
        send_error(413, 'payload_too_large', 'payload too large: request body exceeds limit');
        exit;
    }

    try {
        $payload = json_decode($raw, true, 512, JSON_THROW_ON_ERROR);
    } catch (JsonException $e) {
        send_error(400, 'bad_request', 'invalid request: invalid json: ' . $e->getMessage());
        exit;
    }

    if (!is_array($payload)) {
        send_error(400, 'bad_request', 'invalid request: json must be an object');
        exit;
    }

    $url = isset($payload['url']) && is_string($payload['url']) ? trim($payload['url']) : '';
    if ($url === '') {
        send_error(400, 'bad_request', 'invalid request: url is required');
        exit;
    }

    $width = array_key_exists('width', $payload) && $payload['width'] !== null ? (int) $payload['width'] : null;
    $height = array_key_exists('height', $payload) && $payload['height'] !== null ? (int) $payload['height'] : null;
    $scale = null;
    if (array_key_exists('scale', $payload) && $payload['scale'] !== null) {
        $scale = (float) $payload['scale'];
    }

    if ($width === null && $height === null && $scale === null) {
        send_error(400, 'bad_request', 'invalid request: at least one of width, height, or scale is required');
        exit;
    }

    $fmtRaw = null;
    if (array_key_exists('format', $payload) && $payload['format'] !== null) {
        $fmtRaw = is_string($payload['format']) ? $payload['format'] : (string) $payload['format'];
    }

    $maxDownload = env_int('MAX_DOWNLOAD_BYTES', 26214400);
    $dlTimeout = env_int('DOWNLOAD_TIMEOUT_SECS', 15);
    $maxScale = env_float('MAX_SCALE', 10.0);
    $outputDir = env_str('OUTPUT_DIR', '/data/images');
    $jpegQuality = env_int('JPEG_QUALITY', 85);
    $jpegQuality = max(1, min(100, $jpegQuality));

    $totalStart = now_ms();

    $dlStart = now_ms();
    [$bytes, ] = fetch_url($url, $maxDownload, $dlTimeout);
    $downloadMs = now_ms() - $dlStart;

    $sourceCompressed = strlen($bytes);

    try {
        $outFmt = ($fmtRaw !== null && trim($fmtRaw) !== '')
            ? parse_output_format($fmtRaw)
            : detect_output_format($bytes);
    } catch (InvalidArgumentException $e) {
        send_error(400, 'bad_request', $e->getMessage());
        exit;
    }

    $im = @imagecreatefromstring($bytes);
    if ($im === false) {
        send_error(415, 'unsupported_media_type', 'unsupported media type: could not decode source image: invalid data');
        exit;
    }

    $srcW = imagesx($im);
    $srcH = imagesy($im);
    if ($srcW === 0 || $srcH === 0) {
        imagedestroy($im);
        send_error(415, 'unsupported_media_type', 'unsupported media type: source image has zero dimension');
        exit;
    }

    [$tw, $th] = calculate_target($srcW, $srcH, $width, $height, $scale, $maxScale);

    $resizeStart = now_ms();
    $dst = imagecreatetruecolor($tw, $th);
    if ($dst === false) {
        imagedestroy($im);
        send_error(500, 'internal_error', 'internal error: could not allocate destination image');
        exit;
    }
    imagealphablending($dst, false);
    imagesavealpha($dst, true);
    $transparent = imagecolorallocatealpha($dst, 0, 0, 0, 127);
    imagefill($dst, 0, 0, $transparent);
    imagealphablending($dst, true);
    imagesavealpha($dst, true);
    if (!imagecopyresampled($dst, $im, 0, 0, 0, 0, $tw, $th, $srcW, $srcH)) {
        imagedestroy($im);
        imagedestroy($dst);
        send_error(500, 'internal_error', 'internal error: resize failed');
        exit;
    }
    imagedestroy($im);

    // JPEG: fondo blanco como conversión RGB sin alpha (similar a to_rgb8)
    if ($outFmt['ext'] === 'jpg') {
        $jpegCanvas = imagecreatetruecolor($tw, $th);
        if ($jpegCanvas === false) {
            imagedestroy($dst);
            send_error(500, 'internal_error', 'internal error: could not allocate jpeg canvas');
            exit;
        }
        $white = imagecolorallocate($jpegCanvas, 255, 255, 255);
        imagefill($jpegCanvas, 0, 0, $white);
        imagealphablending($jpegCanvas, true);
        imagecopyresampled($jpegCanvas, $dst, 0, 0, 0, 0, $tw, $th, $tw, $th);
        imagedestroy($dst);
        $dst = $jpegCanvas;
    }

    $resizeMs = now_ms() - $resizeStart;

    if (!is_dir($outputDir) && !@mkdir($outputDir, 0775, true) && !is_dir($outputDir)) {
        imagedestroy($dst);
        send_error(500, 'internal_error', 'internal error: could not ensure output dir');
        exit;
    }

    $filename = uuid_v4() . '.' . $outFmt['ext'];
    $fullPath = rtrim($outputDir, DIRECTORY_SEPARATOR) . DIRECTORY_SEPARATOR . $filename;

    $saveStart = now_ms();
    try {
        write_image($dst, $fullPath, $outFmt['ext'], $jpegQuality);
    } catch (Throwable $e) {
        imagedestroy($dst);
        send_error(500, 'internal_error', 'internal error: encode failed: ' . $e->getMessage());
        exit;
    }
    imagedestroy($dst);
    $saveMs = now_ms() - $saveStart;

    if (!is_file($fullPath)) {
        send_error(500, 'internal_error', 'internal error: could not create output file');
        exit;
    }

    $sizeBytes = filesize($fullPath);
    if ($sizeBytes === false) {
        send_error(500, 'internal_error', 'internal error: could not stat output file');
        exit;
    }

    $totalMs = now_ms() - $totalStart;

    $sourceDecoded = $srcW * $srcH * 4;
    $outputDecoded = $tw * $th * 4;
    $outputEncoded = $sizeBytes;
    $peak = $sourceCompressed + $sourceDecoded + $outputDecoded + $outputEncoded;

    send_json(200, [
        'path' => $fullPath,
        'filename' => $filename,
        'width' => $tw,
        'height' => $th,
        'size_kb' => bytes_to_kb($sizeBytes),
        'format' => $outFmt['ext'],
        'content_type' => $outFmt['ctype'],
        'processing_time_ms' => $totalMs,
        'timing' => [
            'download_ms' => $downloadMs,
            'resize_ms' => $resizeMs,
            'save_ms' => $saveMs,
            'total_ms' => $totalMs,
        ],
        'memory_kb' => [
            'source_compressed_kb' => bytes_to_kb($sourceCompressed),
            'source_decoded_kb' => bytes_to_kb($sourceDecoded),
            'output_decoded_kb' => bytes_to_kb($outputDecoded),
            'output_encoded_kb' => bytes_to_kb($outputEncoded),
            'peak_estimate_kb' => bytes_to_kb($peak),
        ],
    ]);
}
