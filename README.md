# image_proccesor (monorepo)

- **Rust** (API principal): [rust/](rust/)
- **PHP** (réplica para benchmarks): [php/](php/)
- **Loadtest (k6)**: [loadtest/](loadtest/)

## Docker: Rust + PHP con el mismo `rust/.env`

Desde la raíz del repositorio:

```bash
cp .env.example rust/.env
# editá rust/.env (API_TOKEN y el resto; mismo archivo para ambos servicios)

docker compose up -d --build
```

- **Rust**: [http://127.0.0.1:8080](http://127.0.0.1:8080) — salida en `./data/rust`
- **PHP**: [http://127.0.0.1:8081](http://127.0.0.1:8081) — salida en `./data/php`

Solo un servicio:

```bash
docker compose up -d --build image_proccesor_rust
docker compose up -d --build image_proccesor_php
```

Pruebas de carga: [loadtest/k6/README.md](loadtest/k6/README.md) (`BASE_URL` `8080` vs `8081`, mismo `API_TOKEN` que en `rust/.env`).
