# sigma-catalog

Product catalog for Sigma Tactical Group. Stores simple SKUs and composite items (kits/bundles), with a server-rendered web UI and JSON API.

Repository: https://github.com/sigmatactical-org/catalog

Shared site chrome comes from [sigma-theme](https://github.com/sigmatactical-org/sigma-theme).

## Features

- **Simple SKUs** — standalone products with code, name, category, and active flag
- **Composite items** — bundles made of other SKUs with per-component quantities
- **Web UI** — browse, create, edit, and delete SKUs
- **JSON API** — programmatic CRUD for integration behind [sigma-identity](https://github.com/sigmatactical-org/identity)

## Configuration

| Variable | Purpose |
|----------|---------|
| `PORT` | Listen port (default `8080`) |
| `CATALOG_DATA_PATH` | JSON database path (default `data/catalog.json`) |

## Data model

Each SKU has:

- `sku_code` — human-readable identifier (unique)
- `name`, optional `description`, optional `category`
- `kind` — `simple` or `composite`
- `active` — boolean
- `components` — for composite SKUs only: `[{ "sku_id", "quantity" }, …]`

Simple SKUs must have an empty `components` array. Composite SKUs require at least one component referencing existing SKU ids. Cycles and self-references are rejected.

## API

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/skus` | List all SKUs |
| `GET` | `/skus/{id}` | Get one SKU |
| `POST` | `/skus` | Create SKU (JSON) |
| `PUT` | `/skus/{id}` | Update SKU |
| `DELETE` | `/skus/{id}` | Delete SKU |

Example create simple SKU:

```json
{
  "sku_code": "WIDGET-01",
  "name": "Widget",
  "description": "Standard widget",
  "category": "parts",
  "kind": "simple",
  "active": true,
  "components": []
}
```

Example create composite SKU:

```json
{
  "sku_code": "KIT-01",
  "name": "Starter kit",
  "kind": "composite",
  "active": true,
  "components": [
    { "sku_id": "<uuid-of-part-a>", "quantity": 2 },
    { "sku_id": "<uuid-of-part-b>", "quantity": 1 }
  ]
}
```

### Behind sigma-identity

Point identity at this service, for example:

```bash
IDENTITY_PROXY_TARGET=http://127.0.0.1:8080/
```

Browser clients call `/api/skus` on the identity host (with session + CSRF); identity forwards the request with a Bearer token attached.

## Development

```bash
./scripts/prepare-local.sh
cargo run -p sigma-catalog
```

From the sigma workspace:

```bash
cd sigma/commerce/catalog && ./scripts/prepare-local.sh && cargo run -p sigma-catalog
# or prepare all commerce services:
(cd sigma/commerce && ./scripts/prepare-local.sh)
```

Open http://localhost:8080

## Docker

Release is in **`.github/workflows/release.yml`** when configured. Locally:

```bash
./scripts/docker-build.sh
docker build -f Dockerfile build/image
```

Mount a volume at `/app/data` (or set `CATALOG_DATA_PATH`) so catalog data persists across restarts.

## License

MIT OR Apache-2.0
