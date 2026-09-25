# User Interface & Visual Design Specification — Auralis

## 1. Filosofi Desain
Desain visual Auralis berfokus pada **utilitarianisme**, **kecepatan akses**, dan **densitas informasi tinggi**. Antarmuka dirancang agar seorang gamer dapat mengubah keseimbangan audio dalam hitungan detik di sela-sela permainan tanpa terganggu elemen visual yang lambat atau animasi dekoratif yang membebani GPU.

### Prinsip Utama
- **Zero Latency Feel**: Interaksi slider, tombol mute, dan dial ChatMix harus merespons instan tanpa *input lag*.
- **Dark-First Aesthetic**: Tema gelap pekat (*matte carbon*) yang menyatu serasi dengan tema sistem Windows 11 dan aplikasi komunikasi seperti Discord.
- **Hardware-Inspired Ergonomics**: Kontrol audio mengadopsi metafora konsol mixer fisik profesional dengan meteran sinyal (*VU Meter*) responsif.

---

## 2. Design Tokens & Visual Hierarchy

### 2.1. Color Palette (Dark Theme)
```text
Surface & Backgrounds:
  - Surface-0 (App Background) : #111317 (Deep Obsidian)
  - Surface-1 (Card / Strip)    : #181B20 (Dark Carbon)
  - Surface-2 (Elevated Panels) : #22262E (Muted Slate)
  - Border / Divider           : #2E3440 (Subtle Graphite)

Brand & Channel Accents:
  - Game Channel Accent        : #00E5FF (Electric Cyan)
  - Chat Channel Accent        : #FFB300 (Warm Amber)
  - Mic Channel Accent         : #00E676 (Spring Mint)
  - Master / General Accent    : #7C4DFF (Deep Violet)

Feedback & Signal Meters:
  - Signal Normal (-60 to -12) : #00E676 (Green)
  - Signal Warm (-12 to -3)    : #FFEA00 (Yellow)
  - Signal Peak / Clip (> -3)  : #FF1744 (Vivid Red)
```

### 2.2. Tipografi
- **Font Utama**: Segoe UI Variable Text (font native Windows 11) untuk teks antarmuka dan label.
- **Font Numerik**: Cascadia Code / Segoe UI Variable Monospace untuk angka desibel (dB), nilai frekuensi (Hz), dan persentase volume guna mencegah layout bergetar (*jittering*) saat angka berubah.

---

## 3. Tata Letak Jendela Utama (Main Mixer Window)

Jendela utama berukuran kompak tetap: **820 × 540 piksel** (dapat diubah skalanya secara horizontal, tetapi memiliki rasio ideal untuk tampilan mixer 4-kanal).

```text
┌──────────────────────────────────────────────────────────────────────────────────┐
│  [◉] Auralis Audio Studio                                              —  □  ✕  │
├──────────────────────────────────────────────────────────────────────────────────┤
│  Hardware Output: [ Headphones (Realtek High Definition Audio)               ▾ ] │
├───────────────────────────────┬──────────────────────────────────────────────────┤
│         PLAYBACK STRIPS       │                     ROUTING                      │
├───────────────┬───────────────┼──────────────────────────────────────────────────┤
│   🎮 GAME     │   💬 CHAT     │                   CHATMEX DIAL                   │
│               │               │                                                  │
│   ┌───────┐   │   ┌───────┐   │               Game           Chat                │
│   │ ▮   ▮ │   │   │ ▮   ▮ │   │                ◀━━━━━━━●━━━━━━▶                  │
│   │ ▮   ▮ │   │   │ ▮   ▮ │   │                     [ 0 dB ]                     │
│   │ ▮   ▮ │   │   │ ▮   ▮ │   │                                                  │
│   │ ▮   ▮ │   │   │ ▮   ▮ │   ├──────────────────────────────────────────────────┤
│   │ ▮   ▮ │   │   │ ▮   ▮ │   │              MICROPHONE STUDIO                   │
│   │   ●   │   │   │   ●   │   │                                                  │
│   │   │   │   │   │   │   │   │  Device: [ HyperX QuadCast (USB)              ▾] │
│   │   │   │   │   │   │   │   │  Level : ━━━━━━━●━━━━━━ [ +2.5 dB ] [MUTE]       │
│   │   │   │   │   │   │   │   │  Filters: [✓] Noise Gate  [✓] Compressor         │
│   └───────┘   │   └───────┘   │                                                  │
│    -2.0 dB    │    0.0 dB     ├──────────────────────────────────────────────────┤
│    [ MUTE ]   │    [ MUTE ]   │                 MASTER OUTPUT                    │
│    [EQ: FPS]  │   [EQ: Voice] │  Volume : ━━━━━━━━━━━━━● [ -1.0 dB ] [MUTE]      │
├───────────────┴───────────────┴──────────────────────────────────────────────────┤
│  ● Engine: Active | Sample Rate: 48,000 Hz | Buffer: 10 ms | CPU: 0.2% | RAM: 18MB│
└──────────────────────────────────────────────────────────────────────────────────┘
```

---

## 4. Rincian Komponen Antarmuka

### 4.1. Channel Strip Vertikal (Game & Chat)
Setiap strip saluran audio memiliki elemen modular berikut dari atas ke bawah:
1. **Header Saluran**: Ikon dan label berwarna (Cyan untuk Game, Amber untuk Chat).
2. **Dual Stereo VU Meter**: Bar vertikal ganda (kiri dan kanan) yang diperbarui secara halus pada 60 FPS langsung dari *shared memory*.
3. **Volume Fader**: Slider geser vertikal dengan penanda titik tengah (0 dB unity gain) dan rentang dari `-60 dB` hingga `+6 dB`.
4. **Mute Button**: Tombol berstatus ganda dengan indikator warna merah cerah saat aktif.
5. **EQ Quick Drawer**: Tombol pill kecil untuk memilih preset instan atau membuka canvas visualizer EQ.

### 4.2. ChatMix Dual-Bias Control
- **Kontrol**: Slider horizontal besar dengan *tactile center detent* (titik henti lembut pada posisi 50:50).
- **Label Feedback**: Menampilkan rasio numerik dinamis, misalnya `Game 100% : Chat 70%`.
- **Indikasi Visual**: Gradasi warna transisi dari Cyan di sisi kiri ke Amber di sisi kanan.

### 4.3. Parametric Equalizer Visualizer (Modal Drawer)
Ketika tombol `[EQ]` ditekan, laci samping atau panel atas meluas secara halus memperlihatkan canvas kurva frekuensi:
- **Rentang Sumbu-X**: Logaritmik dari 20 Hz hingga 20,000 Hz.
- **Rentang Sumbu-Y**: Linear dari -12 dB hingga +12 dB.
- **Node Poin Interaktif**: 10 node titik yang dapat digeser secara bebas menggunakan kursor:
  - Geser vertikal: Mengubah **Gain**.
  - Geser horizontal: Mengubah **Frequency**.
  - Scroll mouse wheel pada node: Mengubah faktor **Q (Bandwidth)**.
- **RTA (Real-Time Spectrum Analyzer)**: Bayangan spektrum sinyal transparan di belakang kurva filter yang menggambarkan intensitas frekuensi audio yang sedang dimainkan.

---

## 5. System Tray & Micro-Interactions

### 5.1. Siklus Jendela & Penghematan Memori
- Mengklik tombol silang `[✕]` pada jendela tidak menghentikan aplikasi, melainkan menyembunyikan jendela ke *Windows Notification Area (System Tray)*.
- Ketika berada di tray, rendering surface (Direct2D/GPU context) dihancurkan, menurunkan alokasi VRAM hingga mendekati 0 MB.

### 5.2. Menu Klik Kanan System Tray
Menu konteks tray menyediakan tindakan cepat tanpa membuka jendela utama:
```text
┌─────────────────────────────────┐
│ Auralis v1.0                    │
├─────────────────────────────────┤
│ Open Mixer                      │
│ Quick Output: Headphones (USB)  │
│ ------------------------------- │
│ [✓] Mute Microphone             │
│ [ ] Mute All Sounds             │
│ ------------------------------- │
│ Restart Audio Engine            │
│ Settings                        │
│ ------------------------------- │
│ Exit Completely                 │
└─────────────────────────────────┘
```

### 5.3. Keyboard Shortcuts (Global Hotkeys)
Sistem menyediakan pendaftaran hotkey Windows tingkat rendah (opsional):
- `Ctrl + Alt + Arrow Right`: Menggeser ChatMix 5% ke arah Chat.
- `Ctrl + Alt + Arrow Left` : Menggeser ChatMix 5% ke arah Game.
- `Ctrl + Alt + M`          : Mute/Unmute microphone virtual seketika dengan feedback suara bip halus (*subtle audio chime*).
