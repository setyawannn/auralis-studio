# AI Coding Agent Directive — Auralis

Dokumen ini berisi instruksi operasional, batas teknis absolut (*non-negotiable constraints*), dan panduan kontribusi kode bagi Agen AI (seperti Cursor, Windsurf, Claude Code, GitHub Copilot) saat mengimplementasikan fitur atau memperbaiki bug di repositori Auralis.

---

## 1. Misi Utama & Filosofi Proyek
Auralis adalah proyek re-engineering dari SteelSeries Sonar yang berfokus murni pada kinerja, minimalis, dan keandalan sistem audio Windows.
- **Tujuan**: Menyediakan perutean audio gaming (Game, Chat, Mic) dengan latensi <5 ms dan konsumsi memori <30 MB RAM.
- **Karakteristik**: Local-first, native Windows, tanpa akun, tanpa analitik/telemetri, tanpa Chromium/Electron.

---

## 2. Batas Teknis Absolut (Non-Negotiable Constraints)

Agen AI dilarang keras melanggar aturan-aturan berikut saat membuat atau memodifikasi kode:

1. **Aturan Jalur Audio Real-Time**:
   - Dilarang menambahkan alokasi memori dinamis di dalam loop atau callback audio (`Box::new`, `Vec::push`, `String::format`, `alloc()`).
   - Dilarang menggunakan *synchronous mutex* (`std::sync::Mutex`, `parking_lot::Mutex`, `RwLock`) pada thread pemrosesan WASAPI. Gunakan struktur data lock-free (misalnya antrean SPSC dari crate `rtrb`) atau operasi atomik.
   - Dilarang memanggil operasi I/O pemblokir (*file read/write*, *network call*, *blocking pipe*) di dalam thread audio.
   - Dilarang menggunakan runtime async seperti Tokio di dalam thread audio real-time. Tokio hanya diizinkan untuk antarmuka RPC atau tugas latar belakang non-audio.
2. **Aturan Antarmuka & Runtime**:
   - Dilarang menyarankan atau mengintegrasikan kerangka kerja berbasis web (Electron, Tauri WebView, Chromium Embedded Framework, Node.js). Antarmuka harus menggunakan UI toolkit native yang hemat memori (Slint UI dengan backend Direct2D/Femtovg atau Win32 murni).
3. **Aturan Telemetri & Jaringan**:
   - Dilarang menyertakan pustaka pelaporan analitik, pelacakan pengguna jarak jauh, atau koneksi socket keluar secara sembunyi-sembunyi.
4. **Aturan Isolasi Kernel**:
   - Kode kernel C++ driver harus seminimal mungkin; jangan memindahkan logika DSP yang kompleks ke dalam driver kernel. Pemrosesan DSP harus tetap berada di user mode service.

---

## 3. Struktur Direktori Workspace

```text
auralis/
├── Cargo.toml                   # Cargo workspace root
├── driver/                      # Kernel-mode Virtual Audio Driver (WDK C++)
│   ├── sysvad/                  # SysVAD WaveRT fork
│   ├── auralis_driver.inf       # Driver package setup INF file
│   └── CMakeLists.txt           # Build configuration untuk MSBuild / WDK
├── crates/
│   ├── auralis-core/            # Types, ring buffer definitions, shared atomic state
│   ├── auralis-dsp/             # Biquad filter, ChatMix calculation, soft limiter
│   ├── auralis-wasapi/          # Wrapper audio client WASAPI event-driven (windows-rs)
│   ├── auralis-engine/          # Headless audio daemon & MMCSS audio routing loop
│   ├── auralis-ipc/             # Named Pipe server/client protocol & Shared Memory
│   └── auralis-ui/              # Slint UI application & Windows system tray handler
├── packaging/
│   ├── wix/                     # WiX Toolset installer installer definition (.wxs)
│   └── nsis/                    # Skrip NSIS untuk pembuatan single executable setup
└── docs/                        # Dokumentasi teknis & spesifikasi
```

---

## 4. Pola Implementasi Rust yang Diwajibkan

### 4.1. Double-Buffering Parameter DSP
Ketika UI mengubah parameter filter EQ atau volume, worker thread harus menghitung koefisien baru di luar thread audio, kemudian menukarnya secara atomik:

```rust
// CONTOH POLA BENAR: Lock-free parameter swapping
use std::sync::atomic::{AtomicPtr, Ordering};
use std::ptr;

pub struct SharedBiquadConfig {
    active_coefficients: AtomicPtr<BiquadCoefficients>,
}

impl SharedBiquadConfig {
    pub fn update(&self, new_config: Box<BiquadCoefficients>) {
        let new_ptr = Box::into_raw(new_config);
        let old_ptr = self.active_coefficients.swap(new_ptr, Ordering::AcqRel);
        // Deallocasi pointer lama dilakukan di worker thread non-realtime
        if !old_ptr.is_null() {
            unsafe { drop(Box::from_raw(old_ptr)); }
        }
    }

    #[inline(always)]
    pub fn read_current(&self) -> &BiquadCoefficients {
        unsafe { &*self.active_coefficients.load(Ordering::Acquire) }
    }
}
```

### 4.2. Penanganan Error WASAPI & Hotplug
Jangan biarkan program panik saat pemutar suara dicabut. Handle pesan kesalahan perangkat:
```rust
// CONTOH POLA BENAR: Penanganan diskoneksi audio
match audio_client.GetBuffer(&mut p_data, num_frames) {
    Ok(_) => { /* proses rendering */ },
    Err(e) if e.code() == AUDCLNT_E_DEVICE_INVALIDATED || e.code() == AUDCLNT_E_RESOURCES_INVALIDATED => {
        tracing::warn!("Physical audio device invalidated. Triggering recovery state machine.");
        state_notifier.send_recovery_signal();
    },
    Err(e) => {
        tracing::error!("Unexpected WASAPI error: {:?}", e);
    }
}
```

---

## 5. Standar Testing & Verifikasi

Sebelum menyatakan sebuah tugas selesai, Agen AI harus memverifikasi hal berikut:
1. **Unit Test DSP**: Seluruh filter IIR Biquad harus memiliki pengujian stabilitas numerik untuk frekuensi batas (20 Hz dan 20 kHz) guna memastikan tidak terjadi kondisi NaN atau *infinite feedback*.
2. **Zero-Glitch Profiling**: Pengujian streaming audio lokal selama setidaknya 30 detik pada kondisi CPU stress tidak boleh mencatatkan *buffer underrun* (*xrun count = 0*).
3. **Clippy Compliance**: Jalankan `cargo clippy --all-targets -- -D warnings` tanpa ada peringatan yang tertinggal.
4. **Memory Leak Check**: Pastikan proses `auralis-engine` tidak meningkatkan alokasi memori privat saat volume terus-menerus digeser.
