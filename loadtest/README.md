# Load testing (local vs Sprites)

- Main script: [k6/resize.js](k6/resize.js)
- How to run: [k6/README.md](k6/README.md)
- k6 JSON exports: `results/` (`*.json` ignored by git)

## Clearing output (`OUTPUT_DIR`)

Resize writes files to disk. Before or after long stress runs:

**PowerShell (monorepo Docker volumes):**

```powershell
Remove-Item -Recurse -Force "d:\work\trevco\image_proccesor\data\rust\*" -ErrorAction SilentlyContinue
Remove-Item -Recurse -Force "d:\work\trevco\image_proccesor\data\php\*" -ErrorAction SilentlyContinue
```

**Rust-only** from `rust/` with local compose: `./data\*`.

**On the Sprite (bash):**

```bash
rm -f "$HOME/image_data"/* 2>/dev/null
# or whatever path you use for OUTPUT_DIR
```

In the monorepo Docker Compose, `./data/rust` and `./data/php` map to `OUTPUT_DIR`.

## After the run: report

- **Narrative + tables:** [SUMMARY.md](SUMMARY.md) (Rust vs PHP from your latest JSON exports).
- **Charts (Chart.js, static file):** open [report.html](report.html) in a browser.

If you rerun k6 and get new `results/*.json`, update the numbers in `report.html` (`const R` / `const P`) or add a small script to read the JSON and regenerate the page.

## Extra targets (e.g. Sprites)

Add another column or section in [SUMMARY.md](SUMMARY.md) when you export results for a third `BASE_URL`.
