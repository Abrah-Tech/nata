# Deploy Nata di VPS (NixOS, tanpa Docker)

Nata terdiri dari dua bagian yang di-deploy terpisah:

- **`apps/web`** — Next.js static export (`out/`), dilayani langsung oleh nginx.
- **`apps/api`** — binary Rust (`nata-api`), berjalan sebagai systemd service di
  `127.0.0.1:3001`.

Database ada di **Neon** (Postgres managed). Konfigurasi host (unit systemd,
nginx, ACME, direktori release) ada di repo
[nixos](https://github.com/arifinoid/nixos): `nix/modules/nixos/vps-nata.nix`.

---

## Bentuk deployment

```
GitHub Actions (deploy.yml, setelah CI hijau)
  ├─ cargo build --release -p nata-api       (ubuntu-24.04, glibc 2.39)
  ├─ NEXT_PUBLIC_API_URL=https://nata.arifinoid.web.id/api bun run build
  ├─ stage: bin/nata-api + out/ + migrations/ + deploy/vps/grant.sql
  ├─ rsync → /srv/nata/releases/<sha>/
  ├─ symlink /srv/nata/current → release baru (atomik)
  ├─ systemctl start nata-migrate           (sqlx migrate + grant.sql)
  ├─ systemctl restart nata-api
  └─ health check /healthz → gagal? rollback ke release sebelumnya

Host:
  nginx :443 ── /        → /srv/nata/current/out
             ├─ /api/*   → 127.0.0.1:3001 (prefix /api di-strip)
             └─ /healthz → 127.0.0.1:3001 (deploy + monitoring)
```

API sengaja di bawah `/api` supaya tidak menabrak halaman statis yang namanya
sama (`/dashboard`, `/stock`). Browser tetap same-origin, jadi tidak butuh
CORS di production.

---

## 0. Yang perlu disiapkan

- VPS **x86_64** dengan konfigurasi NixOS dari repo `nixos` (host `vps`).
  Modul `vps-nata` membuat user `nata`, `/srv/nata`, unit systemd,
  vhost nginx, dan sertifikat ACME. `/etc/nata.env` diisi manual di langkah 1.
- DNS: A record `nata.arifinoid.web.id` → IP VPS.
- Project **Neon** (free tier cukup) dengan Postgres 16/17.
- Secret GitHub di repo ini: `VPS_HOST` dan `VPS_SSH_KEY` (private key yang
  public-nya ada di `/root/.ssh/authorized_keys` VPS).

---

## 1. Database Neon (sekali saja)

Neon memberi satu role owner (mis. `neondb_owner`). Nata memakai dua role:

| Role | Dipakai oleh | Hak |
|---|---|---|
| owner (`neondb_owner`) | `nata-migrate` saja | DDL: migrations + grants |
| `nata_app` | `nata-api` | DML + execute fungsi auth, RLS aktif |

Ambil **Direct connection string** (host tanpa `-pooler`) dari dashboard Neon,
lalu siapkan `/etc/nata.env`:

```bash
# di mesin lokal
cp deploy/vps/env.example /tmp/nata.env
# isi DATABASE_URL (nata_app), MIGRATE_DATABASE_URL (owner),
# NATA_APP_PASSWORD (sama dengan password DATABASE_URL), JWT_SECRET
ssh root@VPS 'install -m 600 /dev/stdin /etc/nata.env' < /tmp/nata.env
rm /tmp/nata.env
```

Role `nata_app` belum ada saat pertama; `grant.sql` yang membuatnya (butuh
`NATA_APP_PASSWORD`). Jadi `DATABASE_URL` boleh ditulis lebih dulu.

`JWT_SECRET` harus acak, misal:

```bash
openssl rand -hex 32
```

> **Catatan Neon:** `sslmode=require` wajib. Kalau connection string membawa
> `channel_binding=...`, hapus parameter itu (sqlx tidak memakainya).

---

## 2. Deploy

Push ke `main` → CI hijau → `Deploy` berjalan otomatis (`workflow_run`).

```bash
git push origin main
```

Deploy pertama (atau re-deploy manual):

```bash
gh workflow run deploy.yml --repo Abrah-Tech/nata --ref main
```

Verifikasi:

```bash
ssh root@VPS 'systemctl status nata-api nata-migrate --no-pager'
curl -fsS https://nata.arifinoid.web.id/healthz
```

### Login pertama

OTP dikirim lewat `LogOtpSender`, jadi kode hanya muncul di journal:

```bash
ssh root@VPS 'journalctl -u nata-api -f'
# cari baris: development OTP code contact=... code=123456
```

Release build selalu mengabaikan `DEV_OTP_CODE`; jangan set variabel itu.

### Rollback

Workflow otomatis mengembalikan symlink ke release sebelumnya kalau migrasi,
restart, atau health check gagal.

Manual:

```bash
ssh root@VPS '
  set -e
  ln -sfn /srv/nata/releases/<sha-lama> /srv/nata/current.new
  mv -Tf /srv/nata/current.new /srv/nata/current
  systemctl restart nata-api
'
```

Tiga release terakhir disimpan; sisanya dipangkas setelah deploy sukses.

---

## 3. Backup

Neon menyimpan backup/PITR sendiri di dashboard (retention tergantung plan),
jadi tidak ada timer `pg_dump` di VPS. Kalau butuh dump lokal, jalankan manual:

```bash
ssh root@VPS 'pg_dump "$MIGRATE_DATABASE_URL" -Fc -f /root/nata-$(date +%F).dump'
```

---

## 4. Catatan

- **Tanpa Docker**: host VPS tidak punya Docker. Binary Rust + nginx native.
- **Migrasi tidak otomatis saat API start**: `main` tidak memanggil
  `nata_db::migrate()`; deploy menjalankan `nata-migrate` lebih dulu.
- **RLS**: semua tabel tenant memakai `force row level security`. `nata_app`
  bukan owner, jadi policy tetap berlaku; fungsi `SECURITY DEFINER` untuk
  lookup pra-tenant dijalankan sebagai owner.
- **Tauri shell tidak di-deploy**: `apps/shell` hanya untuk desktop.
