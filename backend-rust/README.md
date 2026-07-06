# ChatVault Rust backend

This is an API-compatible Rust replacement for the original Kotlin/Spring ChatVault backend.

Current target:

- PostgreSQL persistence
- MinIO/S3 archive storage
- WhatsApp `.txt` and `.zip` import
- Frontend-compatible `/api` routes
- Static frontend serving

Not yet ported:

- IMAP/email importer

## Configuration

Required:

- `DATABASE_URL`
- `S3_ENDPOINT`
- `S3_BUCKET`
- `S3_ACCESS_KEY`
- `S3_SECRET_KEY`

Optional:

- `BIND` default `0.0.0.0:8080`
- `S3_REGION` default `us-east-1`
- `CHATVAULT_PUBLIC_DIR` default `/app/public`
- `CHATVAULT_SCRATCH_DIR` default `/tmp/chatvault`
- `CHATVAULT_IMPORT_DIR` default `/opt/chatvault/import`
- `CHATVAULT_BUCKET_PROVIDER` default `s3`, can be `filesystem`
- `CHATVAULT_BUCKET_ROOT` used only for filesystem storage
- `CHATVAULT_MSGPARSER_DATEFORMAT`

