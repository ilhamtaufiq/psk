# Proyek Selepas Kerja

Aplikasi web manajemen proyek sederhana: autentikasi, daftar & pengelolaan proyek, detail proyek, dan berkas proyek.

- **Backend:** Rust (axum 0.8, sqlx + SQLite, argon2)
- **Frontend:** Astro (SSR, adapter Node)
- **Deploy:** Coolify (Docker Compose), domain `psk.cjr.my.id`

## Fitur

| Fitur | Keterangan |
| --- | --- |
| Autentikasi | Daftar, masuk, keluar. Password di-hash dengan Argon2. Sesi memakai token acak, disimpan sebagai SHA-256 di database, cookie `httpOnly` berlaku 30 hari. |
| Manajemen proyek | Buat, ubah, hapus proyek. Status: Perencanaan, Sedang berjalan, Ditunda, Selesai. Filter per status. |
| Detail proyek | Status, tanggal mulai/selesai, deskripsi, waktu dibuat & diubah, jumlah berkas. |
| Berkas proyek | Unggah (maks. 25 MB per berkas), unduh, hapus. Berkas disimpan di disk dengan nama UUID. |

Setiap pengguna hanya bisa melihat proyek dan berkas miliknya sendiri.

## Struktur

```
.
├── docker-compose.yml        # Dipakai Coolify
├── backend/                  # API Rust
│   ├── Cargo.toml
│   ├── Dockerfile
│   ├── migrations/001_init.sql
│   └── src/
│       ├── main.rs           # router & konfigurasi
│       ├── auth.rs           # register, login, logout, extractor AuthUser
│       ├── projects.rs       # CRUD proyek
│       ├── files.rs          # unggah, unduh, hapus berkas
│       └── error.rs          # format error JSON
└── frontend/                 # Astro SSR
    ├── Dockerfile
    ├── astro.config.mjs
    └── src/
        ├── middleware.ts     # validasi sesi & proteksi halaman
        ├── lib/              # klien API, cookie, format tanggal/ukuran
        ├── layouts/ components/ styles/
        └── pages/
            ├── login.astro, register.astro, projects/...   # halaman
            └── api/...                                     # endpoint form (proxy ke backend)
```

Arsitektur: peramban hanya bicara dengan Astro. Astro memanggil backend Rust lewat jaringan internal (`API_URL`). Backend tidak dipublikasikan ke internet.

## Menjalankan lokal

Prasyarat: Rust (stable), Node 22+, pnpm 10.

Terminal 1 — backend (port 8080):

```bash
cd backend
DATABASE_URL=sqlite://data/app.db UPLOAD_DIR=data/uploads BIND_ADDR=127.0.0.1:8080 cargo run
```

Terminal 2 — frontend (port 4321):

```bash
cd frontend
pnpm install
API_URL=http://127.0.0.1:8080 pnpm dev
```

Buka http://localhost:4321 lalu daftar akun baru.

## Konfigurasi (environment variable)

| Variabel | Service | Default | Keterangan |
| --- | --- | --- | --- |
| `DATABASE_URL` | backend | `sqlite://data/app.db` | File SQLite; dibuat otomatis, migrasi dijalankan saat start |
| `UPLOAD_DIR` | backend | `data/uploads` | Folder penyimpanan berkas |
| `BIND_ADDR` | backend | `127.0.0.1:8080` | Di container: `0.0.0.0:8080` |
| `RUST_LOG` | backend | `info` | Level log |
| `API_URL` | frontend | `http://127.0.0.1:8080` | Alamat backend yang dilihat frontend |
| `HOST`, `PORT` | frontend | — | Bawaan adapter Node; di image: `0.0.0.0:4321` |

## API backend (ringkas)

Semua endpoint selain `auth/register` dan `auth/login` memerlukan header `Authorization: Bearer <token>`.

| Method | Path | Fungsi |
| --- | --- | --- |
| POST | `/api/auth/register` | Daftar, mengembalikan `{ token, user }` |
| POST | `/api/auth/login` | Masuk |
| POST | `/api/auth/logout` | Hapus sesi |
| GET | `/api/auth/me` | Pengguna saat ini |
| GET | `/api/projects?status=` | Daftar proyek |
| POST | `/api/projects` | Buat proyek |
| GET / PUT / DELETE | `/api/projects/{id}` | Detail / ubah / hapus proyek |
| GET | `/api/projects/{id}/files` | Daftar berkas |
| POST | `/api/projects/{id}/files` | Unggah berkas (multipart, field `file`) |
| GET / DELETE | `/api/files/{id}` | Unduh / hapus berkas |
| GET | `/api/health` | Cek hidup |

## Deploy ke Coolify

1. Pastikan kode ada di repositori Git (GitHub/GitLab/Gitea) dan `backend/Cargo.lock` ikut ter-commit.
2. Di Coolify: **New Resource → Docker Compose** → pilih repositori dan branch. Base directory: `/` (root repo), Compose file: `docker-compose.yml`.
3. Buka service **frontend** dan isi domain: `https://psk.cjr.my.id`, port `4321`. Coolify mengurus sertifikat HTTPS.
4. Jangan isi domain untuk service **backend**; biarkan internal.
5. Deploy. Volume `psk-data` menyimpan database dan berkas. Jangan dihapus saat redeploy.
6. Cek: buka `https://psk.cjr.my.id`, daftar akun, buat proyek, dan unggah satu berkas.

Catatan:
- Cookie sesi otomatis memakai atribut `Secure` ketika permintaan datang lewat HTTPS (membaca `X-Forwarded-Proto` dari proxy Coolify).
- Backup: salin volume `psk-data` secara berkala (berisi `app.db` dan folder `uploads/`).
- Untuk skala lebih besar, SQLite bisa diganti PostgreSQL; kode sqlx perlu disesuaikan.

## Batasan saat ini

- Belum ada berbagi proyek ke pengguna lain (satu proyek = satu pemilik).
- Belum ada reset password dan verifikasi email.
- Belum ada tugas (task) di dalam proyek; fokus pada detail, status, dan berkas.
