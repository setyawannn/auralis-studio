# Technical Architecture Specification — Auralis

## 1. High-Level Architecture Overview
Arsitektur Auralis memisahkan *driver kernel*, *engine audio real-time*, dan *antarmuka grafis pengguna (GUI)* ke dalam tiga domain independen untuk memastikan keandalan mutlak pada audio gaming.

```text
┌────────────────────────────────────────────────────────────────────────┐
│                        Auralis Client (UI & Tray)                      │
│                    Rust + Slint Native UI (User Mode)                  │
│                                                                        │
│   [Mixer View]   [EQ Graphical Canvas]   [ChatMix]   [Device Picker]   │
└────────────────────────────────────┬───────────────────────────────────┘
                                     │ Windows Named Pipes (RPC / Control)
                                     │ + Shared Memory (VU Meters / Fast State)
                                     ▼
┌────────────────────────────────────────────────────────────────────────┐
│                     Auralis Audio Engine Daemon                        │
│                 Rust (windows-rs + WASAPI Event-Driven)                │
│                                                                        │
│   ┌────────────────────────┐  ┌───────────────────┐  ┌─────────────┐   │
│   │ Device Watcher (MMDev) │  │  Config Manager   │  │ Ring Buffers│   │
│   └───────────┬────────────┘  └─────────┬─────────┘  └──────┬──────┘   │
│               │                         │                   │          │
│   ┌───────────▼─────────────────────────▼───────────────────▼──────┐   │
│   │           Real-time Processing Pipeline (MMCSS "Pro Audio")    │   │
│   │    Biquad EQ  ·  Gain Staging  ·  ChatMix  ·  Peak Limiter     │   │
│   └───────────────────────────────┬────────────────────────────────┘   │
└───────────────────────────────────┼────────────────────────────────────┘
          WASAPI Ingestion Callback │  WASAPI Render Callback
                                    ▼
┌───────────────────────────────────────┐  ┌─────────────────────────────┐
│      Auralis Virtual Audio Driver     │  │   Physical Audio Endpoints  │
│          (C++ / WDK WaveRT SysVAD)    │  │   (Realtek, USB DAC, etc.)  │
│                                       │  │                             │
│ [Auralis Game]   [Auralis Chat] [Mic] │  │  WASAPI Exclusive / Shared  │
└───────────────────────────────────────┘  └─────────────────────────────┘
```

---

## 2. Rincian Modul & Pemilihan Bahasa

### 2.1. Kernel Driver Layer (`auralis-driver.sys`)
- **Bahasa**: C / C++ (Windows Driver Kit - WDK).
- **Arsitektur Dasar**: Fork dari Microsoft SysVAD (WaveRT Virtual Audio Device Driver).
- **Tanggung Jawab**:
  - Mendaftarkan virtual playback adapter yang mengekspos 2 endpoint rendering: `Auralis Game Audio` dan `Auralis Chat Audio`.
  - Mendaftarkan virtual capture adapter yang mengekspos 1 endpoint recording: `Auralis Microphone Audio`.
  - Menyediakan memory pin circular buffer DMA sintetis untuk streaming paket audio tanpa memakan latensi bus perangkat keras.

### 2.2. Audio Engine Core (`auralis-engine`)
- **Bahasa**: Rust (edisi 2021/2024).
- **Tanggung Jawab**:
  - Berjalan sebagai proses latar belakang (*user session daemon*).
  - Menginisialisasi WASAPI Capture pada endpoint virtual menggunakan `AUDCLNT_STREAMFLAGS_EVENTCALLBACK`.
  - Membaca sample PCM `f32`, memasukkannya ke ring buffer lock-free, melakukan DSP (Gain, EQ, Limiter), dan merendernya ke endpoint fisik aktif.
  - Mendengarkan notifikasi IMMNotificationClient untuk perubahan status perangkat fisik (*unplug*, *default device switch*).

### 2.3. Antarmuka Grafis (`auralis-ui`)
- **Bahasa**: Rust + Slint UI Toolkit (berjalan dengan backend Direct2D/Femtovg).
- **Tanggung Jawab**:
  - Menampilkan dashboard kontrol visual dengan konsumsi RAM <35 MB.
  - Berkomunikasi asinkron dengan engine melalui IPC.
  - Membebaskan memori grafis saat diminimalkan ke system tray.

---

## 3. Concurrency & Threading Model

Sistem menerapkan model isolasi thread yang ketat. Setiap siklus audio real-time dilindungi dari interferensi sistem operasi.

```text
+-------------------+      +-------------------+      +-------------------+
|  WASAPI Capture   |      |  WASAPI Capture   |      |  WASAPI Capture   |
|   (Game Thread)   |      |   (Chat Thread)   |      |   (Mic In Thread) |
+---------+---------+      +---------+---------+      +---------+---------+
          | SPSC Ring                | SPSC Ring                | SPSC Ring
          | Buffer                   | Buffer                   | Buffer
          v                          v                          v
+-------------------------------------------------------------------------+
|                  Real-time DSP & Mixer Thread (MMCSS)                   |
|                                                                         |
|  - Pop audio frame (10ms)                                               |
|  - Read atomic coefficients (Double-buffered Biquad parameters)         |
|  - Apply Volume & Equal-Power ChatMix                                   |
|  - Sum stereo buffers to Master bus                                     |
|  - Evaluate Peak Limiter                                                |
|  - Write to WASAPI Physical Render Buffer                               |
|  - Publish Peak/RMS Meters to Atomic Shared Memory                      |
+-------------------------------------------------------------------------+
                                     ^
                                     | Lock-Free Parameter Swap
+------------------------------------+------------------------------------+
|                      Config & IPC Worker Thread                         |
|                                                                         |
|  - Handle Named Pipe requests from UI                                   |
|  - Calculate Biquad filter coefficients (sin/cos/pow)                   |
|  - Swap pointer to active audio filter configuration                    |
|  - Write state changes to disk (debounced TOML writer)                  |
+-------------------------------------------------------------------------+
```

### 3.1. Aturan Khusus Thread Real-Time Audio
Thread yang menangani callback WASAPI dan DSP mixer mematuhi invariant berikut:
1. **Zero Heap Allocation**: Tidak memanggil alokasi memori dinamis (`alloc`, `realloc`, `free`, `Box::new`, `Vec::push`).
2. **Zero Blocking Synchronization**: Tidak menggunakan `std::sync::Mutex`, `rwlock`, atau *blocking condition variables*.
3. **Zero System & File I/O**: Tidak melakukan logging ke disk, operasi socket, atau pembacaan file.
4. **Lock-Free Communication**: Menggunakan antrean *Single-Producer Single-Consumer (SPSC)* ring buffer (`rtrb` crate) untuk aliran audio dan atomic pointer swap untuk pembaruan parameter DSP.
5. **Thread Priority**: Didaftarkan ke Windows Multimedia Class Scheduler Service (MMCSS) menggunakan API `AvSetMmThreadCharacteristicsW(w!("Pro Audio"), &mut task_index)`.

---

## 4. Digital Signal Processing (DSP) Pipeline

### 4.1. Format Audio Internal
Semua jalur audio dinormalisasi ke format internal:
- **Sample Rate**: 48,000 Hz.
- **Data Type**: 32-bit floating point (`f32`) interleaved stereo.
- **Buffer Size**: 480 samples per frame (10 ms per siklus pemrosesan).

### 4.2. Parametric Equalizer (Biquad Filter)
Setiap band EQ menggunakan filter IIR biquad bertipe *Direct Form II Transposed* untuk stabilitas numerik tinggi pada komputasi `f32`:
```text
y[n] = b0 * x[n] + s1[n-1]
s1[n] = b1 * x[n] - a1 * y[n] + s2[n-1]
s2[n] = b2 * x[n] - a2 * y[n]
```
Koefisien filter dihitung pada thread worker non-real-time saat slider digeser dan ditransmisikan ke thread audio secara atomik.

### 4.3. Algoritma ChatMix (Constant-Power)
Untuk menjaga kestabilan energi suara saat membagi porsi Game dan Chat, gain dihitung dengan formulasi:
\[
\theta = \frac{\pi}{4} \times (x + 1.0) \quad \text{untuk } x \in [-1.0, 1.0]
\]
\[
\text{Gain}_{\text{Game}} = \cos(\theta), \quad \text{Gain}_{\text{Chat}} = \sin(\theta)
\]
Ketika \(x = 0.0\) (posisi tengah seimbang), kedua saluran menerima faktor pengali \( \approx 0.7071 \) (-3 dB), menghasilkan daya total output yang konstan tanpa kliping.

### 4.4. Master Soft Limiter
Sebelum ditulis ke perangkat keras fisik, buffer sinyal melewati deteksi puncak. Jika nilai absolut sample melebihi ambang batas \(-0.5 \text{ dBFS}\), transfer fungsi kompresi kurva lunak (*tanh soft-clipping*) diaktifkan untuk mencegah distorsi digital yang tajam.

---

## 5. Komunikasi Antar-Proses (IPC)

Sistem menggunakan arsitektur IPC ganda untuk memisahkan lalu lintas kontrol dan data telemetri berkecepatan tinggi:

1. **Windows Named Pipe (`\\.\pipe\auralis-control`)**:
   - Pola: Request-Response RPC berbasis binary protocol berbingkai (*length-prefixed binary/bincode*).
   - Digunakan untuk: Perubahan konfigurasi, pemilihan perangkat fisik, penggantian preset EQ, dan sinyal penutupan aplikasi.
2. **Windows Shared Memory (`Local\AuralisAudioState`)**:
   - Pola: Shared memory berbasis `CreateFileMappingW` yang dipetakan langsung ke struktur data memory-mapped C-compatible.
   - Menggunakan atomik `AtomicU32` untuk membaca level puncak sinyal (*VU meter*) saluran Game, Chat, Mic, dan Master pada kecepatan 60 FPS tanpa overhead pengiriman paket pesan.

---

## 6. Device Watcher & Failure Recovery State Machine

Sistem mengimplementasikan mesin status untuk menangani siklus hidup perangkat audio fisik di Windows secara tangguh:

```text
       [ INITIALIZING ]
              │
              ▼
        [ RUNNING ] ◄────────────────────────────────────────┐
              │                                              │
              │ AUDCLNT_E_DEVICE_INVALIDATED                 │ Perangkat Pulih
              │ atau OnDeviceStateChanged (Unplug)           │ (Fade-In 30ms)
              ▼                                              │
       [ RECONNECTING ] ─────────────────────────────────────┤
              │                                              │
              │ Endpoint pilihan tidak ditemukan             │
              ▼                                              │
       [ FALLBACK MODE ] ────────────────────────────────────┘
              │ (Gunakan Windows Default Multimedia Endpoint)
              │
              │ Tidak ada perangkat audio sama sekali
              ▼
     [ SILENT SAFE MODE ] (Drop stream ke dummy sink, sleep 250ms interval)
```

Jika terjadi perubahan perangkat, sistem audio menerapkan kurva *fade-out* 20 ms sebelum melepas handle WASAPI lama dan *fade-in* 30 ms setelah handle baru diaktifkan guna mengeliminasi suara "letupan" (*audio pop/click*).
