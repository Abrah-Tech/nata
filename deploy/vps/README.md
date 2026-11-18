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

## 3. Backup dan restore

Neon menyimpan backup/PITR sendiri di dashboard (retention tergantung plan),
jadi tidak ada timer `pg_dump` di VPS. Yang tidak otomatis adalah bukti bahwa
dump itu benar-benar bisa dipakai. `deploy/vps/backup.sh` menjalankan drill:
dump, restore ke database sementara, bandingkan, lalu hapus database itu.

Yang dibandingkan:

- **skema** — nama, tipe, dan nullability tiap kolom. `_sqlx_migrations` ikut,
  jadi level migrasi yang berbeda akan gagal;
- **RLS** — tabel mana yang memaksa RLS, dan setiap policy beserta perintah dan
  ekspresinya. Batas tenansi harus bertahan dari restore; policy yang hilang
  berarti kebocoran lintas-tenant, bukan sekadar kehilangan data;
- **jumlah baris** persis per tabel;
- **isi tiap kolom** — kolom `numeric` dijumlahkan persis, kolom lain di-hash per
  baris lalu hash-nya dijumlahkan. Jadi nama atau tanggal yang berubah tertangkap
  juga, bukan hanya amount.

`pg_dump` yang lebih tua dari server ditolak di awal, karena dump pendek baru
terasa saat dipakai.

Sumber hanya pernah dibaca. Ada perbedaan → script berhenti dengan diff dan exit
code 1. Lolos → database scratch di-drop dan dump dihapus, kecuali `--keep`.

### Lokal

```bash
docker compose up -d db
DATABASE_URL=postgres://nata:nata@127.0.0.1:5432/nata sqlx migrate run
DATABASE_URL=postgres://nata:nata@127.0.0.1:5432/nata deploy/vps/backup.sh
```

### Di VPS

`backup.sh` ikut dalam release (`/srv/nata/current/deploy/vps/`). Pakai role
owner, karena drill membuat database sementara:

```bash
ssh root@VPS '
  /srv/nata/current/deploy/vps/backup.sh \
    --url "$MIGRATE_DATABASE_URL" --keep --output /root
'
```

`--self-test` mengubah satu baris di salinan restore lalu menuntut perbandingan
gagal. Jalankan sekali-sekala untuk memastikan drill-nya sendiri masih
berfungsi, bukan cuma script-nya masih jalan.

### Restore sungguhan

Drill di atas membuktikan dump masuk ke Postgres dan datanya utuh. Untuk restore
sebenernya, jalur tercepat di Neon adalah membuat branch atau project baru dari
PITR di dashboard, lalu menunjuk `DATABASE_URL` ke sana. Kalau dashboard tidak
memadai, restore dump ke database kosong:

```bash
pg_restore --dbname="$MIGRATE_DATABASE_URL" --no-owner --no-privileges \
  /root/nata-<tanggal>.dump
```

Setelah restore, jalankan `grant.sql` lagi kalau role `nata_app` belum ada di
target, dan cocokkan `_sqlx_migrations` sebelum `nata-api` diswitch.

---

## 4. Rotasi `JWT_SECRET`

Ganti secret saat bocor atau saat ganti kredensial, bukan karena ada jadwal.
API menerima dua secret sekaligus: secret sekarang dipakai untuk menandatangani
token baru, secret sebelumnya masih diterima selama masa hidup token
(`JWT_TTL_SECONDS`, default 24 jam). Jadi rotasi tidak mengeluarkan semua orang.

```bash
# di VPS, atas nama root
SECRET_LAMA=$(grep '^JWT_SECRET=' /etc/nata.env | cut -d= -f2-)
SECRET_BARU=$(openssl rand -hex 32)

sed -i "s|^JWT_SECRET=.*|JWT_SECRET=$SECRET_BARU|" /etc/nata.env
if grep -q '^JWT_SECRET_PREV=' /etc/nata.env; then
  sed -i "s|^JWT_SECRET_PREV=.*|JWT_SECRET_PREV=$SECRET_LAMA|" /etc/nata.env
else
  echo "JWT_SECRET_PREV=$SECRET_LAMA" >> /etc/nata.env
fi

systemctl restart nata-api
```

Token yang diterbitkan sebelum rotasi tetap dipakai sampai habis masa hidupnya;
token baru ditandatangani dengan secret baru. Kalau `HASH_PEPPER` belum diset,
set it juga sekarang — tanpa itu, code OTP dan undangan yang masih pending ikut
hangus tiap rotasi.

Setelah satu `JWT_TTL_SECONDS` lewat (dan semua token lama mati), hapus
`JWT_SECRET_PREV` lalu restart. Biarkan keduanya sama-sama ada selamanya
berarti secret lama tetap bisa dipakai membuat token baru, yang membatalkan
tujuan rotasi.

Kalau `JWT_SECRET_PREV` sama dengan `JWT_SECRET`, API menulis peringatan dan
mengabaikan salah satunya.

---

## 5. Catatan

- **Tanpa Docker**: host VPS tidak punya Docker. Binary Rust + nginx native.
- **Migrasi tidak otomatis saat API start**: `main` tidak memanggil
  `nata_db::migrate()`; deploy menjalankan `nata-migrate` lebih dulu.
- **RLS**: semua tabel tenant memakai `force row level security`. `nata_app`
  bukan owner, jadi policy tetap berlaku; fungsi `SECURITY DEFINER` untuk
  lookup pra-tenant dijalankan sebagai owner.
- **Tauri shell tidak di-deploy**: `apps/shell` hanya untuk desktop.
