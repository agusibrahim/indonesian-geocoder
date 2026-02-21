# Indonesian Geocoder & Places API 🇮🇩

**Alternatif Self-Hosted Bebas Biaya untuk Google Maps Geocoding & Place Search API!**
Repository: [https://github.com/agusibrahim/indonesian-geocoder](https://github.com/agusibrahim/indonesian-geocoder)

Proyek ini adalah API *Reverse Geocoding* dan *Places Search* offline dan super cepat menggunakan Rust. Dibangun khusus untuk memproses data geospasial wilayah Indonesia (Provinsi, Kabupaten/Kota, Kecamatan, hingga Kelurahan/Desa) dari file GeoJSON. Berhenti membayar tagihan API yang mahal hanya untuk mengidentifikasi lokasi user di Indonesia! Cukup jalankan *binary* ringan ini di server VPS Anda sendiri (*self-hosted*), dan nikmati pencarian tanpa batas kueri (*unlimited requests*).

> **Credit Data:** Sumber data koordinat poligon batas wilayah Indonesia yang digunakan pada proyek ini diambil dari repositori luar biasa karya **[@fityannugroho/idn-area-boundary](https://github.com/fityannugroho/idn-area-boundary)**.

Sistem ini sangat efisien karena beroperasi menggunakan data polygon WKB (*Well-Known Binary*) yang telah disimplifikasi di dalam database SQLite.

## Fitur Utama

- **Google Maps API Alternative:** Dirancang memiliki perilaku kemiripan fungsi seperti Google Maps Geocoding API dan Places API (Proximity Search), namun sepenuhnya *offline* dan gratis!
- **Cepat & Ringan:** Ditulis dengan bahasa Rust, footprint RAM yang sangat minim.
- **Reverse Geocoding:** Konversi titik koordinat GPS (`lat`, `lng`) menjadi nama wilayah Kelurahan yang akurat, lengkap dengan jarak pengguna dari titik pusat desa (*Haversine Distance*). Menggunakan algoritma *Point-in-Polygon* (PIP) yang dipercepat dengan pre-filter Bounding Box.
- **Proximity Search:** Mencari lokasi di Indonesia hanya dengan teks (misal `"majalaya karawang"`). Jika disertakan koordinat user, sistem otomatis mengurutkan hasil pencarian dari yang jaraknya paling dekat dengan user.
- **Auto-Download DB:** Server secara pintar akan mendownload *database* awal jika file SQLite belum tersedia di dalam sistem saat server di-*run*.

## Endpoint API

### 1. Reverse Geocoding
**URL:** `GET /api/v1/geocode/reverse`
**Query Params:**
- `lat` (float): Latitude
- `lng` (float): Longitude

**Contoh:**
```bash
curl "http://localhost:3000/api/v1/geocode/reverse?lat=-6.321293&lng=107.361877"
```

**Response:**
```json
{
  "success": true,
  "data": {
    "level": "village",
    "id": "32.15.21.2008",
    "name": "Bengle",
    "location_detail": {
      "province": "Jawa Barat",
      "regency": "Kabupaten Karawang",
      "district": "Majalaya",
      "village": "Bengle"
    },
    "full_name": "Kelurahan Bengle, Kecamatan Majalaya, Kabupaten Karawang, Jawa Barat",
    "lat": -6.3253600124931975,
    "lng": 107.35743087293734,
    "distance_meters": 668
  },
  "error": null
}
```

### 2. Places Search
**URL:** `GET /api/v1/places/search`
**Query Params:**
- `q` (string): Keyword pencarian nama lokasi
- `lat` (float, opsional): Latitude user, untuk prioritas lokasi terdekat
- `lng` (float, opsional): Longitude user, untuk prioritas lokasi terdekat
- `limit` (int, opsional, default 10): Jumlah output

**Contoh:**
```bash
curl "http://localhost:3000/api/v1/places/search?q=majalaya%20karawang&limit=1"
```

---

## 🛠 Instalasi dan Pembangunan (Build)

### Menjalankan Server API (Rust)

1. Pastikan Anda telah menginstal `Rust` dan `Cargo`.
2. Lakukan build/run di mode *release* untuk mendapat kecepatan penuh.

```bash
cargo run --release
```
Server akan berjalan di port `3000`.

### Re-build / Membangun Ulang Database Geospasial

Proyek ini menggunakan *database* tunggal `indonesia_area.db` (SQLite). Jika Anda ingin memperbarui atau membuat ulang database ini dari file *raw* GeoJSON:

1. Pastikan Anda memiliki direktori `data/` dengan hierarki berikut:
   ```text
   data/
   ├── provinces/
   ├── regencies/
   ├── districts/
   └── villages/
   ```
2. Anda harus menginstall *library Python* yang dibutuhkan terlebih dahulu:
   ```bash
   pip install shapely
   ```
3. Jalankan *script parser*-nya:
   ```bash
   python create_db.py
   ```
   *Proses ini memakan waktu beberapa menit karena sistem harus membaca ~83.000 file JSON, melakukan simplifikasi geometri (membuang poligon kecil/lurus yang tidak berguna via algoritma Ramer-Douglas-Peucker), menghitung bounding box, dan mengonversinya ke WKB Blob SQLite.*

**Kustomisasi Akurasi Poligon (Simplifikasi):**
Di dalam file `create_db.py` terdapat konstanta `SIMPLIFY_TOLERANCE`. Saat ini bernilai `0.0002` (Akurasi toleransi ±20 meter). Ubah nilai ini jika Anda ingin database yang ukurannya lebih ringan atau lebih detail.

---

## Deployment & CI/CD
Proyek ini dilengkapi dengan GitHub Actions (`.github/workflows/release.yml`) yang secara otomatis mem-_build_ aplikasi (*cross-compile*) secara statis dan meng-upload file eksekusi (*binary file*) dalam format `tar.gz` di tab **Releases** GitHub setiap kali ada tag rilis (misal `v1.0.0`).

Dukungan arsitektur:
- Linux AMD64 (x86_64)
- Linux ARM64 (aarch64)
- macOS Intel
- macOS Apple Silicon (M1/M2/M3)
- Windows x86_64
