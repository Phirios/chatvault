# ChatVault Rust backend

This is an API-compatible Rust replacement for the original ChatVault backend.

Current target:

- PostgreSQL persistence
- MinIO/S3 archive storage
- WhatsApp `.txt` and `.zip` import
- Frontend-compatible `/api` routes
- Static frontend serving
- Optional IMAP/email importer

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
- `CHATVAULT_EMAIL_ENABLED` default `false`
- `CHATVAULT_EMAIL_HOST`
- `CHATVAULT_EMAIL_PORT` default `993`
- `CHATVAULT_EMAIL_USERNAME`
- `CHATVAULT_EMAIL_PASSWORD`
- `CHATVAULT_EMAIL_FIXED_DELAY_MS` default `10000`
- `CHATVAULT_EMAIL_SUBJECT_STARTS_WITH` default `chat-vault,Conversa do WhatsApp com`
