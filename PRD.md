# Product Requirements Document (PRD) — Auralis

## 1. Executive Summary & Vision
**Auralis** adalah utilitas desktop audio Windows berkinerja tinggi yang dirancang khusus untuk memecahkan fragmentasi perutean audio gaming dan komunikasi. Auralis mereplikasi nilai inti dari *SteelSeries Sonar*—yaitu pemisahan saluran audio virtual, kontrol ChatMix instan, parametric equalizer tingkat lanjut, dan pemrosesan mikrofon lokal—tanpa membawa *bloatware*, sistem akun, telemetri, atau konsumsi memori tinggi khas aplikasi berbasis Electron.

### Nilai Inti Produk
- **Ultra-Lightweight**: Penggunaan memori terisolasi (<30 MB RAM) dan beban CPU terabaikan (<0.3% idle).
- **Zero-Bloat Philosophy**: Tidak ada akun, tidak ada telemetri, tidak ada browser tertanam, tidak ada perekam video latar belakang, dan tidak ada toko digital.
- **Local-First & Offline**: Berjalan 100% lokal di mesin pengguna tanpa ketergantungan server awan.
- **Single-Click UX**: Pengalaman instalasi "Next-Next-Finish" dengan pendaftaran perangkat virtual otomatis.

---

## 2. Target Pengguna & Persona
1. **Competitive Gamer**: Membutuhkan pemisahan audio game dan suara teman (Discord), pengaturan EQ spesifik frekuensi langkah kaki musuh (*footsteps*), dan latensi ultra-rendah.
2. **Streamer / Content Creator**: Membutuhkan perutean audio audio mandiri tanpa konfigurasi kabel virtual pihak ketiga yang rumit.
3. **Power User & Minimalist**: Pengguna Windows yang menyukai fitur Sonar tetapi membenci ekosistem SteelSeries GG yang berat dan lambat.

---

## 3. Cakupan Fitur (Scope)

### In-Scope (Fase MVP)
- **Virtual Audio Endpoints**: Pendaftaran perangkat playback virtual `Auralis Game` dan `Auralis Chat`, serta capture virtual `Auralis Microphone`.
- **Master Routing & Mixer**: Perutean sinyal PCM dari saluran virtual ke satu perangkat output fisik (headphone/DAC/speaker).
- **Hardware Mic Ingestion**: Penangkapan mikrofon fisik ke saluran pemrosesan suara sebelum diteruskan ke `Auralis Microphone`.
- **Hardware ChatMix**: Slider crossfade daya konstan (*constant-power crossfade*) untuk menyeimbangkan intensitas suara Game vs Chat secara real-time.
- **Dual Parametric EQ**: 10-band IIR Biquad Equalizer terpisah untuk saluran Game dan Chat beserta preset bawaan (*Flat*, *FPS Footsteps*, *Immersion*, *Voice Clarity*).
- **Microphone Clean Chain**: Noise gate sederhana, saturasi/kompresor ringan, gain staging, dan limiter anti-clipping.
- **Tray & Session Management**: Jendela aplikasi dapat diminimalkan ke system tray tanpa memutus rantai rendering audio.

### In-Scope (Fase Pasca-MVP: v1.1 - v1.2)
- Saluran virtual tambahan: `Auralis Media` dan `Auralis Aux`.
- Integrasi modul *AI Noise Suppression* berbasis model lokal berukuran kecil (RNNoise FFI).
- Sidetone/Mic Monitoring latensi rendah ke output fisik.
- Dukungan hotkey global Windows untuk ChatMix dan toggle mute.

### Out-of-Scope (Prinsip Anti-Bloat)
- Perekaman klip game, pemotongan video, dan overlay layar.
- Integrasi pencahayaan RGB periferal perangkat keras.
- Autentikasi web, registrasi akun, sinkronisasi cloud, dan analitik jarak jauh.
- Toko aplikasi atau feed promosi game.

---

## 4. Kebutuhan Fungsional (Functional Requirements)

| ID | Modul | Deskripsi Kebutuhan |
| :--- | :--- | :--- |
| **FR-01** | Virtual Devices | Sistem harus mendaftarkan perangkat virtual WDM audio pada Windows Device Manager dengan nama ramah pengguna. |
| **FR-02** | WASAPI Ingestion | Audio engine harus menangkap audio format PCM (shared mode) dari endpoint virtual secara event-driven dengan buffer 10 ms. |
| **FR-03** | Physical Output | Audio engine harus meneruskan hasil mixing ke endpoint fisik aktif via WASAPI Low-Latency Shared/Exclusive mode. |
| **FR-04** | ChatMix Engine | Slider posisi rentang `[-1.0, 1.0]` harus mengatur rasio gain Game dan Chat menggunakan kurva *equal-power* tanpa penurunan volume perseptual di titik tengah. |
| **FR-05** | Parametric EQ | Pengguna dapat mengatur frekuensi (20 Hz - 20 kHz), Q factor (0.1 - 10.0), dan Gain (-12 dB hingga +12 dB) per band. Perubahan parameter diterapkan secara lock-free. |
| **FR-06** | Mic Processing | Jalur mikrofon harus menyediakan: input gain digital (+/- 24 dB), noise gate dengan threshold/hysteresis, dan peak limiter (-0.1 dBFS). |
| **FR-07** | Device Hotplug | Ketika headphone fisik dicabut/dipasang kembali, engine harus melakukan fallback otomatis ke default device dalam kurun waktu <500 ms tanpa crash. |
| **FR-08** | Persistent State | Seluruh state mixer, volume, dan preset disimpan dalam format file lokal TOML saat terjadi perubahan dengan mekanisme *debounced write* (300 ms). |

---

## 5. Kebutuhan Non-Fungsional (Non-Functional Requirements)

- **NFR-01 (Latensi)**: Latensi pemrosesan audio internal (*pipeline delay*) tidak boleh melebihi 5 milidetik pada sample rate standar 48 kHz / 16-bit atau 24-bit.
- **NFR-02 (Konsumsi RAM)**: Total penggunaan memori kerja proses latar belakang (*engine*) tidak boleh melebihi 25 MB RAM. Penggunaan UI saat aktif tidak boleh melebihi 35 MB RAM.
- **NFR-03 (Beban CPU)**: Penggunaan CPU pada kondisi idle (playback normal 2-channel tanpa GUI terbuka) harus berada di bawah 0.3% pada prosesor setara Intel Core i5 generasi ke-10 atau lebih baru.
- **NFR-04 (Keandalan Real-Time)**: Audio engine dilarang melakukan alokasi heap dinamis (*malloc/free*) atau operasi I/O pemblokir pada thread audio WASAPI guna mencegah *buffer underrun* (*glitch/xrun*).
- **NFR-05 (Privasi & Keamanan)**: Aplikasi tidak membuka port soket jaringan publik dan tidak melakukan transmisi paket keluar (*zero network telemetry*).
- **NFR-06 (Portabilitas & Packaging)**: Installer tunggal berekstensi `.exe` yang menyertakan sertifikat penandatanganan driver yang valid, mendukung pemasangan/pencabutan bersih via Windows Settings.

---

## 6. Kriteria Keberhasilan Rilis (Release Acceptance Criteria)
1. **Soak Test Audio**: Audio diputar nonstop selama 12 jam melalui saluran Game dan Chat bersamaan tanpa terjadi desinkronisasi atau peningkatan pemakaian RAM (*memory leak*).
2. **Stress Test Disconnect**: Mencabut kabel USB DAC saat game berjalan tidak menyebabkan game freeze atau crash pada service Auralis.
3. **Instalasi Murni**: Pengguna baru dapat menyelesaikan instalasi dalam waktu kurang dari 60 detik hanya dengan 3 langkah konfirmasi (*UAC -> Directory -> Finish*).
