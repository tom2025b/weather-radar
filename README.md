# weather-radar

A native Rust GUI app showing live NWS forecast + NOAA radar for Alachua, FL.

**Public domain — see [UNLICENSE](UNLICENSE)**

---

## Features

- **NWS 7-day forecast** pulled directly from `api.weather.gov` — real text forecasts, not third-party summaries
- **Live NOAA radar** — base reflectivity composite (FL/SE region) via the official NOAA WMS endpoint
- **Expandable forecast cards** — click "details" on any period for the full NWS paragraph
- **Temperature color coding** — blue (freezing) → green → yellow → red (hot)
- **Auto-refresh every 5 minutes** in the background
- Manual **Refresh now** button
- Fully compiled Rust binary — fast, no Python or runtime needed

## Screenshot

```
┌─────────────────────────────────────────────────────────┐
│ ⛅ Alachua, FL — NWS + Live Radar   74.0°F  Updated 2m  │
├──────────────────────────┬──────────────────────────────┤
│ 📋 NWS 7-Day Forecast    │ 📡 NOAA Base Reflectivity    │
│                          │                              │
│ ☀️ Today       74°F      │  [live radar image]          │
│ Mostly Sunny             │                              │
│ 💨 S 10 mph              │                              │
│                          │                              │
│ 🌙 Tonight     52°F      │                              │
│ Clear                    │  🔄 Refresh now              │
│ 💨 NW 5 mph              │                              │
│  ...                     │                              │
├──────────────────────────┴──────────────────────────────┤
│ ● All data current.                                     │
└─────────────────────────────────────────────────────────┘
```

## Build

```bash
# Install Rust if needed
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build release binary
cargo build --release

# Run
./target/release/weather-radar
```

Requires OpenGL (Linux: Mesa/X11 or Wayland via eframe).

## Data sources

| Data | Source |
|------|--------|
| Forecast text | `https://api.weather.gov/` (NWS) |
| Radar image | `https://opengeo.ncep.noaa.gov/` (NOAA WMS) |

Both are free, no API key required.

## Related

- [toms-weather-cli](https://github.com/tom2025b/toms-weather-cli) — the fast terminal-only Rust weather tool
- [weather-desktop](https://github.com/tom2025b/weather-desktop) — system tray icon version

## License

Public domain — Unlicense. No attribution required.
