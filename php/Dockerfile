# syntax=docker/dockerfile:1.7
# PHP built-in server + api.php (mismo contrato que el servicio Rust).
FROM php:8.3-cli-bookworm

RUN apt-get update \
 && apt-get install -y --no-install-recommends \
        ca-certificates \
        wget \
        pkg-config \
        libfreetype6-dev \
        libjpeg62-turbo-dev \
        libpng-dev \
        libwebp-dev \
 && docker-php-ext-configure gd --with-freetype --with-jpeg --with-webp \
 && docker-php-ext-install -j"$(nproc)" gd \
 && rm -rf /var/lib/apt/lists/*

WORKDIR /var/www/html

COPY api.php .

# Root: bind-mount ./data/php desde el host suele ser root:writable para cualquier UID.
RUN mkdir -p /data/images

EXPOSE 8080

HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://127.0.0.1:8080/health || exit 1

CMD ["php", "-S", "0.0.0.0:8080", "api.php"]
