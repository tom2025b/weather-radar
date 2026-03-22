# weather-radar

A fast Rust CLI + GUI for live NWS forecasts and NOAA radar — enter any US ZIP code and get the weather instantly, with the radar image URL centred on your location.

**Public domain — [UNLICENSE](UNLICENSE)**

---

## Features

- Enter **any US ZIP code or city name** — no hardcoded location
- **NWS 7-day forecast** direct from `api.weather.gov` — real text, no third-party middleman
- **Live NOAA radar URL** centred on your exact coordinates
- **Roast mode** — brutally honest one-liner about today's conditions
- **Saves output** to `~/weather-<zip>.txt` automatically after every run
- **GUI mode** (when a display is available) — egui window with live radar image + clickable forecast cards
- **Auto text mode** when running over SSH / no display — just works
- No API key required. No account. No tracking.

---

## Quick start

```bash
# Build
cargo build --release

# Run (SSH / terminal — prompts for ZIP)
./target/release/weather-radar

# Pass ZIP directly
./target/release/weather-radar --text 10001

# Any city works too
./target/release/weather-radar --text "Chicago, IL"

# GUI (requires X11 or Wayland)
./target/release/weather-radar
```

---

## Text mode output

```
  Enter ZIP code or city: 90210

  Weather — Beverly Hills, Los Angeles County, California  (34.0901, -118.4065)
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Now: 68.0°F

  Today  68°F  Mostly Sunny
  💨 → W 12 mph   rain   3%   💧 45%
  Mostly sunny skies with a light westerly breeze...

  🔥 Genuinely pleasant. The weather is doing its job. Give it a gold star.

  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Day    Hi    Lo  Forecast                Precip
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
     Sun   72°    55°  Mostly Sunny            rain   2%
     Mon   69°    54°  Partly Cloudy           rain  10%
     ...

  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
  Radar (NOAA — centred on your location):
  https://opengeo.ncep.noaa.gov/geoserver/conus/conus_bref_qcd/ows?...
  ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

  Saved → /home/you/weather-90210.txt
```

---

## Saved files

After every run the full forecast is written to:

```
~/weather-<zip>.txt
```

Examples: `~/weather-10001.txt`, `~/weather-chicago-il.txt`

---

## GUI mode

When a display is present the app launches an egui window:

```
┌─────────────────────────────────────────────────────────┐
│ ⛅  Chicago, IL — NWS + Live Radar   42.0°F  Updated 2m  │
├──────────────────────────┬──────────────────────────────┤
│ 📋 NWS 7-Day Forecast    │ 📡 NOAA Base Reflectivity    │
│                          │                              │
│ ☀️ Today       42°F      │  [live radar image]          │
│ Mostly Cloudy            │                              │
│ 💨 NW 14 mph             │                              │
│  ...                     │  🔄 Refresh now              │
└──────────────────────────┴──────────────────────────────┘
```

- Location search bar accepts any ZIP or city
- Radar tile zoomed to your coordinates
- Auto-refreshes every 5 minutes

---

## Data sources

| Data | Source |
|------|--------|
| Forecast text + 7-day | `https://api.weather.gov/` (NWS) |
| Radar image | `https://opengeo.ncep.noaa.gov/` (NOAA WMS) |
| Geocoding | `https://nominatim.openstreetmap.org/` (OpenStreetMap) |

All free. No API key required.

---

## Dependencies

```toml
eframe / egui   # GUI
reqwest         # HTTP (blocking, rustls)
serde_json      # JSON parsing
image           # PNG decode for radar texture
urlencoding     # URL-safe geocode queries
```

---

## License

Public domain — Unlicense. No attribution required. Do whatever you want with it.
