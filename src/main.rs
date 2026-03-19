/// weather-radar — NWS 7-day forecast + live NOAA radar GUI
/// Location: dynamic via geocoding (default Alachua, FL)
/// Data: api.weather.gov (forecast) + opengeo.ncep.noaa.gov WMS (radar)
///       Nominatim / OpenStreetMap (geocoding)
/// License: Unlicense (public domain)
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

// ── Constants ──────────────────────────────────────────────────────────────

const DEFAULT_LAT: f64 = 29.80997;
const DEFAULT_LON: f64 = -82.4675;
const DEFAULT_LABEL: &str = "Alachua, FL";
const REFRESH_SECS: u64 = 300;
const USER_AGENT: &str = "weather-radar/0.1 (thomaslane2025@gmail.com)";

/// NOAA WMS base-reflectivity composite — CONUS
/// EPSG:4326 axis order for WMS 1.3.0: minLat,minLon,maxLat,maxLon
const RADAR_URL: &str =
    "https://opengeo.ncep.noaa.gov/geoserver/conus/conus_bref_qcd/ows\
     ?service=WMS&version=1.3.0&request=GetMap\
     &layers=conus_bref_qcd\
     &CRS=EPSG:4326\
     &BBOX=27.5,-86.0,31.5,-80.0\
     &WIDTH=640&HEIGHT=427\
     &FORMAT=image/png";

// ── Data types ─────────────────────────────────────────────────────────────

#[derive(Clone, Default)]
struct ForecastPeriod {
    name:              String,
    temperature:       i64,
    unit:              String,
    short_forecast:    String,
    detailed_forecast: String,
    wind_speed:        String,
    wind_direction:    String,
    is_daytime:        bool,
    precip_chance:     Option<i64>,
    icon_url:          String,
    humidity:          Option<i64>,
}

struct AppData {
    // forecast
    periods:         Vec<ForecastPeriod>,
    current_temp:    Option<f64>,
    selected_period: usize,
    // radar
    radar_bytes:     Option<Vec<u8>>,
    radar_dirty:     bool,
    // location
    lat:             f64,
    lon:             f64,
    location_label:  String,
    location_input:  String,
    geocode_error:   Option<String>,
    // status
    status:          String,
    last_updated:    Option<Instant>,
}

impl Default for AppData {
    fn default() -> Self {
        Self {
            periods:         Vec::new(),
            current_temp:    None,
            selected_period: 0,
            radar_bytes:     None,
            radar_dirty:     false,
            lat:             DEFAULT_LAT,
            lon:             DEFAULT_LON,
            location_label:  DEFAULT_LABEL.to_string(),
            location_input:  String::new(),
            geocode_error:   None,
            status:          String::new(),
            last_updated:    None,
        }
    }
}

// ── App struct ─────────────────────────────────────────────────────────────

struct WeatherApp {
    shared:        Arc<Mutex<AppData>>,
    radar_texture: Option<egui::TextureHandle>,
}

impl WeatherApp {
    fn new(_cc: &eframe::CreationContext, shared: Arc<Mutex<AppData>>) -> Self {
        Self { shared, radar_texture: None }
    }
}

// ── Color / display helpers ────────────────────────────────────────────────

fn temp_color(t: i64) -> egui::Color32 {
    match t {
        i64::MIN..=32 => egui::Color32::from_rgb(100, 180, 255),
        33..=50       => egui::Color32::from_rgb(150, 220, 255),
        51..=65       => egui::Color32::from_rgb(160, 240, 160),
        66..=80       => egui::Color32::from_rgb(255, 220, 80),
        81..=95       => egui::Color32::from_rgb(255, 140, 40),
        _             => egui::Color32::from_rgb(255, 60, 60),
    }
}

fn temp_bg_color(t: i64) -> egui::Color32 {
    match t {
        i64::MIN..=32 => egui::Color32::from_rgb(20, 50, 90),
        33..=50       => egui::Color32::from_rgb(18, 48, 85),
        51..=65       => egui::Color32::from_rgb(18, 55, 60),
        66..=80       => egui::Color32::from_rgb(50, 50, 20),
        81..=95       => egui::Color32::from_rgb(65, 35, 10),
        _             => egui::Color32::from_rgb(70, 15, 10),
    }
}

fn wind_arrow(dir: &str) -> &'static str {
    match dir.trim() {
        "N"          => "↑",
        "NNE" | "NE" => "↗",
        "ENE" | "E"  => "→",
        "ESE" | "SE" => "↘",
        "SSE" | "S"  => "↓",
        "SSW" | "SW" => "↙",
        "WSW" | "W"  => "←",
        "WNW" | "NW" | "NNW" => "↖",
        _            => "·",
    }
}

fn precip_label(precip: Option<i64>) -> (String, egui::Color32) {
    match precip {
        Some(p) if p > 20 => (format!("🌧 {}%", p), egui::Color32::from_rgb(80, 160, 255)),
        Some(p)           => (format!("🌧 {}%", p), egui::Color32::GRAY),
        None              => ("🌧 --%".to_string(), egui::Color32::DARK_GRAY),
    }
}

fn short_words(s: &str, n: usize) -> String {
    s.split_whitespace().take(n).collect::<Vec<_>>().join(" ")
}

fn abbrev_day(name: &str) -> String {
    let first = name.split_whitespace().next().unwrap_or(name);
    match first {
        "Tonight" => "Tonite".to_string(),
        "Today" | "This" => "Today".to_string(),
        other => other.chars().take(3).collect(),
    }
}

// ── eframe::App ────────────────────────────────────────────────────────────

impl eframe::App for WeatherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.request_repaint_after(Duration::from_secs(15));

        // ── Upload radar texture if new bytes arrived ───────────────────────
        {
            let mut data = self.shared.lock().unwrap();
            if data.radar_dirty {
                if let Some(bytes) = &data.radar_bytes {
                    if let Ok(img) = image::load_from_memory(bytes) {
                        let rgba   = img.to_rgba8();
                        let width  = rgba.width()  as usize;
                        let height = rgba.height() as usize;
                        let color_img = egui::ColorImage::from_rgba_unmultiplied(
                            [width, height], &rgba,
                        );
                        self.radar_texture = Some(ctx.load_texture(
                            "radar", color_img, egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                data.radar_dirty = false;
            }
        }

        // Snapshot shared state for this frame
        let (periods, current_temp, status, last_updated, selected_period, location_label, geocode_error) = {
            let d = self.shared.lock().unwrap();
            (
                d.periods.clone(),
                d.current_temp,
                d.status.clone(),
                d.last_updated,
                d.selected_period,
                d.location_label.clone(),
                d.geocode_error.clone(),
            )
        };

        // ── Location search bar ────────────────────────────────────────────
        egui::TopBottomPanel::top("location_bar")
            .frame(egui::Frame::default()
                .fill(egui::Color32::from_rgb(15, 40, 80))
                .inner_margin(egui::Margin::same(6.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("📍").size(14.0));
                    let mut trigger = false;
                    {
                        let mut d = self.shared.lock().unwrap();
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut d.location_input)
                                .hint_text("Enter city, state or ZIP…")
                                .desired_width(260.0),
                        );
                        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            trigger = true;
                        }
                    }
                    if ui.button("🔍 Search").clicked() {
                        trigger = true;
                    }
                    if trigger {
                        let query = self.shared.lock().unwrap().location_input.trim().to_string();
                        if !query.is_empty() {
                            let shared = Arc::clone(&self.shared);
                            thread::spawn(move || do_geocode_and_fetch(query, shared));
                        }
                    }
                    if let Some(ref err) = geocode_error {
                        ui.label(
                            egui::RichText::new(format!("⚠ {}", err))
                                .small().color(egui::Color32::from_rgb(255, 100, 100)),
                        );
                    }
                });
            });

        // ── Header ─────────────────────────────────────────────────────────
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::default()
                .fill(egui::Color32::from_rgb(10, 30, 60))
                .inner_margin(egui::Margin::same(8.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!("⛅  {}  —  NWS Forecast + Live Radar", location_label))
                            .size(17.0).strong().color(egui::Color32::WHITE),
                    );
                    ui.separator();
                    if let Some(t) = current_temp {
                        ui.label(
                            egui::RichText::new(format!("{:.1}°F", t))
                                .size(22.0).strong().color(temp_color(t as i64)),
                        );
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if let Some(instant) = last_updated {
                            let s = instant.elapsed().as_secs();
                            let txt = if s < 60 { format!("Updated {}s ago", s) }
                                      else      { format!("Updated {}m ago", s / 60) };
                            ui.label(egui::RichText::new(txt).small().color(egui::Color32::LIGHT_GRAY));
                        } else {
                            ui.label(egui::RichText::new("Fetching…").small().color(egui::Color32::YELLOW));
                        }
                    });
                });
            });

        // ── Status bar ─────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::default()
                .fill(egui::Color32::from_rgb(5, 15, 35))
                .inner_margin(egui::Margin::same(4.0)))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("● {}", status))
                        .small().color(egui::Color32::from_rgb(120, 180, 120)),
                );
            });

        // ── Radar panel (right) ────────────────────────────────────────────
        egui::SidePanel::right("radar_panel")
            .min_width(440.0).max_width(700.0).resizable(true)
            .frame(egui::Frame::default()
                .fill(egui::Color32::from_rgb(5, 10, 25))
                .inner_margin(egui::Margin::same(8.0)))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("📡  Live Radar — {}", location_label))
                        .strong().color(egui::Color32::WHITE),
                );
                ui.separator();

                let avail_h = ui.available_height();
                let radar_h = (avail_h - 48.0).max(100.0) * 0.80;

                if let Some(tex) = &self.radar_texture {
                    let avail_w = ui.available_width();
                    let img_h = (avail_w * (427.0 / 640.0)).min(radar_h);
                    let img_w = img_h * (640.0 / 427.0);
                    ui.image((tex.id(), egui::vec2(img_w, img_h)));
                } else {
                    ui.allocate_ui(egui::vec2(ui.available_width(), radar_h), |ui| {
                        ui.vertical_centered(|ui| {
                            ui.add_space(radar_h / 2.0 - 20.0);
                            ui.label(egui::RichText::new("⏳  Loading radar…").color(egui::Color32::YELLOW));
                            ui.label(egui::RichText::new("(NOAA WMS — may take a few seconds)").small().color(egui::Color32::GRAY));
                        });
                    });
                }

                ui.add_space(8.0);
                if ui.button("🔄  Refresh now").clicked() {
                    let shared = Arc::clone(&self.shared);
                    thread::spawn(move || do_fetch(&shared));
                }
                ui.add_space(4.0);
                ui.label(egui::RichText::new("NOAA opengeo.ncep.noaa.gov + api.weather.gov")
                    .small().color(egui::Color32::DARK_GRAY));
            });

        // ── Forecast panel (center) ────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(8, 18, 40)))
            .show(ctx, |ui| {
                ui.label(egui::RichText::new("📋  NWS 7-Day Forecast").strong().color(egui::Color32::WHITE));
                ui.separator();

                if periods.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(egui::RichText::new("Fetching forecast from api.weather.gov…")
                            .color(egui::Color32::YELLOW));
                    });
                    return;
                }

                // ── TODAY STRIP ────────────────────────────────────────────
                let today = &periods[0];
                let (precip_txt, precip_col) = precip_label(today.precip_chance);
                let warr = wind_arrow(&today.wind_direction);

                egui::Frame::default()
                    .fill(temp_bg_color(today.temperature))
                    .rounding(egui::Rounding::same(10.0))
                    .inner_margin(egui::Margin::same(14.0))
                    .outer_margin(egui::Margin { bottom: 10.0, ..Default::default() })
                    .stroke(egui::Stroke::new(1.5, egui::Color32::from_rgb(60, 100, 180)))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(
                                egui::RichText::new(format!("{}°{}", today.temperature, today.unit))
                                    .size(38.0).strong().color(temp_color(today.temperature)),
                            );
                            ui.separator();
                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("{} — {}", today.name, today.short_forecast))
                                        .size(15.0).strong().color(egui::Color32::WHITE),
                                );
                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(
                                        format!("💨 {} {} {}", warr, today.wind_direction, today.wind_speed)
                                    ).color(egui::Color32::from_rgb(160, 200, 255)));
                                    ui.separator();
                                    ui.label(egui::RichText::new(&precip_txt).color(precip_col));
                                    if let Some(hum) = today.humidity {
                                        ui.separator();
                                        ui.label(egui::RichText::new(format!("💧 {}%", hum))
                                            .color(egui::Color32::from_rgb(100, 200, 200)));
                                    }
                                });
                            });
                        });
                    });

                // ── 7-DAY COMPACT GRID ────────────────────────────────────
                let daytime_idx: Vec<usize> = periods.iter().enumerate()
                    .filter(|(_, p)| p.is_daytime)
                    .map(|(i, _)| i)
                    .take(7)
                    .collect();

                let accent = egui::Color32::from_rgb(255, 200, 50);
                let col_w = 112.0_f32;

                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(10, 22, 48))
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(8.0))
                    .outer_margin(egui::Margin { bottom: 10.0, ..Default::default() })
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            for (col, &di) in daytime_idx.iter().enumerate() {
                                let dp = &periods[di];
                                let night_temp = periods.get(di + 1)
                                    .filter(|p| !p.is_daytime)
                                    .map(|p| p.temperature);
                                let is_sel = selected_period == di || selected_period == di + 1;

                                let cell_bg = if col % 2 == 0 {
                                    egui::Color32::from_rgb(18, 40, 80)
                                } else {
                                    egui::Color32::from_rgb(14, 32, 65)
                                };
                                let stroke = if is_sel {
                                    egui::Stroke::new(2.0, accent)
                                } else {
                                    egui::Stroke::new(0.5, egui::Color32::from_rgb(30, 55, 100))
                                };

                                let resp = egui::Frame::default()
                                    .fill(cell_bg)
                                    .rounding(egui::Rounding::same(6.0))
                                    .inner_margin(egui::Margin::same(6.0))
                                    .stroke(stroke)
                                    .show(ui, |ui| {
                                        ui.set_min_width(col_w);
                                        ui.set_max_width(col_w);
                                        ui.vertical_centered(|ui| {
                                            ui.label(egui::RichText::new(abbrev_day(&dp.name))
                                                .strong().color(egui::Color32::WHITE));
                                            ui.label(egui::RichText::new(format!("{}°", dp.temperature))
                                                .size(15.0).strong().color(temp_color(dp.temperature)));
                                            if let Some(lo) = night_temp {
                                                ui.label(egui::RichText::new(format!("{}°", lo))
                                                    .size(12.0).color(temp_color(lo).linear_multiply(0.75)));
                                            } else {
                                                ui.add_space(14.0);
                                            }
                                            ui.label(egui::RichText::new(short_words(&dp.short_forecast, 2))
                                                .small().color(egui::Color32::LIGHT_GRAY));
                                            let (pt, pc) = precip_label(dp.precip_chance);
                                            ui.label(egui::RichText::new(pt).small().color(pc));
                                        });
                                    });

                                if resp.response.interact(egui::Sense::click()).clicked() {
                                    self.shared.lock().unwrap().selected_period = di;
                                }
                            }
                        });
                    });

                // ── DETAIL PANEL ───────────────────────────────────────────
                let sel_idx = selected_period.min(periods.len().saturating_sub(1));
                let sel = &periods[sel_idx];
                let warr2 = wind_arrow(&sel.wind_direction);
                let (dp_txt, dp_col) = precip_label(sel.precip_chance);

                egui::Frame::default()
                    .fill(egui::Color32::from_rgb(5, 13, 30))
                    .rounding(egui::Rounding::same(8.0))
                    .inner_margin(egui::Margin::same(12.0))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 60, 110)))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let icon = if sel.is_daytime { "☀️" } else { "🌙" };
                            ui.label(egui::RichText::new(format!("{} {}", icon, sel.name))
                                .strong().size(14.0).color(egui::Color32::WHITE));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(format!("{}°{}", sel.temperature, sel.unit))
                                    .strong().size(16.0).color(temp_color(sel.temperature)));
                            });
                        });
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(
                                format!("💨 {} {} {}", warr2, sel.wind_direction, sel.wind_speed)
                            ).small().color(egui::Color32::from_rgb(160, 200, 255)));
                            ui.separator();
                            ui.label(egui::RichText::new(&dp_txt).small().color(dp_col));
                            if let Some(hum) = sel.humidity {
                                ui.separator();
                                ui.label(egui::RichText::new(format!("💧 {}% humidity", hum))
                                    .small().color(egui::Color32::from_rgb(100, 200, 200)));
                            }
                        });
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical()
                            .id_source("detail_scroll")
                            .max_height(150.0)
                            .show(ui, |ui| {
                                let txt = if sel.detailed_forecast.is_empty() {
                                    &sel.short_forecast
                                } else {
                                    &sel.detailed_forecast
                                };
                                ui.label(egui::RichText::new(txt.as_str())
                                    .small().color(egui::Color32::LIGHT_GRAY));
                            });
                    });
            });
    }
}

// ── Geocoding ──────────────────────────────────────────────────────────────

fn geocode(query: &str) -> Result<(f64, f64, String), String> {
    let encoded = urlencoding::encode(query);
    let url = format!(
        "https://nominatim.openstreetmap.org/search?q={}&format=json&limit=1",
        encoded
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())?;

    let resp: serde_json::Value = client.get(&url)
        .send().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let arr = resp.as_array().ok_or("Geocode: unexpected response")?;
    if arr.is_empty() {
        return Err(format!("No results found for \"{}\"", query));
    }
    let first = &arr[0];
    let lat: f64 = first["lat"].as_str().ok_or("missing lat")?
        .parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let lon: f64 = first["lon"].as_str().ok_or("missing lon")?
        .parse().map_err(|e: std::num::ParseFloatError| e.to_string())?;
    let label = first["display_name"].as_str().unwrap_or(query).to_string();
    Ok((lat, lon, label))
}

fn do_geocode_and_fetch(query: String, shared: Arc<Mutex<AppData>>) {
    set_status(&shared, format!("Geocoding: {}…", query));
    match geocode(&query) {
        Ok((lat, lon, label)) => {
            {
                let mut d = shared.lock().unwrap();
                d.lat = lat;
                d.lon = lon;
                d.location_label = label;
                d.geocode_error = None;
            }
            do_fetch(&shared);
        }
        Err(e) => {
            let mut d = shared.lock().unwrap();
            d.geocode_error = Some(e.clone());
            d.status = format!("Geocode error: {}", e);
        }
    }
}

// ── Data fetching ──────────────────────────────────────────────────────────

fn do_fetch(shared: &Arc<Mutex<AppData>>) {
    let (lat, lon) = { let d = shared.lock().unwrap(); (d.lat, d.lon) };

    set_status(shared, "Fetching NWS forecast…".into());
    match fetch_forecast(lat, lon) {
        Ok((temp, periods)) => {
            let mut d = shared.lock().unwrap();
            d.current_temp = Some(temp);
            d.periods = periods;
            d.status = "Forecast OK — fetching radar…".into();
        }
        Err(e) => set_status(shared, format!("Forecast error: {e}")),
    }

    set_status(shared, "Fetching NOAA radar…".into());
    match fetch_radar() {
        Ok(bytes) => {
            let mut d = shared.lock().unwrap();
            d.radar_bytes = Some(bytes);
            d.radar_dirty = true;
            d.status = "All data current.".into();
            d.last_updated = Some(Instant::now());
        }
        Err(e) => {
            let mut d = shared.lock().unwrap();
            d.status = format!("Radar error: {e}");
            d.last_updated = Some(Instant::now());
        }
    }
}

fn set_status(shared: &Arc<Mutex<AppData>>, s: String) {
    shared.lock().unwrap().status = s;
}

fn make_client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(15))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| e.to_string())
}

fn fetch_forecast(lat: f64, lon: f64) -> Result<(f64, Vec<ForecastPeriod>), String> {
    let client = make_client()?;

    let points: serde_json::Value = client
        .get(format!("https://api.weather.gov/points/{},{}", lat, lon))
        .send().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let forecast_url = points["properties"]["forecast"]
        .as_str().ok_or("NWS: missing forecast URL")?.to_string();

    let fc: serde_json::Value = client
        .get(&forecast_url)
        .send().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let raw = fc["properties"]["periods"]
        .as_array().ok_or("NWS: missing periods")?;

    let mut periods = Vec::with_capacity(raw.len());
    let mut current_temp = 0.0_f64;

    for (i, p) in raw.iter().enumerate() {
        let temp = p["temperature"].as_i64().unwrap_or(0);
        if i == 0 { current_temp = temp as f64; }
        periods.push(ForecastPeriod {
            name:              p["name"].as_str().unwrap_or("").to_string(),
            temperature:       temp,
            unit:              p["temperatureUnit"].as_str().unwrap_or("F").to_string(),
            short_forecast:    p["shortForecast"].as_str().unwrap_or("").to_string(),
            detailed_forecast: p["detailedForecast"].as_str().unwrap_or("").to_string(),
            wind_speed:        p["windSpeed"].as_str().unwrap_or("").to_string(),
            wind_direction:    p["windDirection"].as_str().unwrap_or("").to_string(),
            is_daytime:        p["isDaytime"].as_bool().unwrap_or(true),
            precip_chance:     p["probabilityOfPrecipitation"]["value"].as_i64(),
            icon_url:          p["icon"].as_str().unwrap_or("").to_string(),
            humidity:          p["relativeHumidity"]["value"].as_i64(),
        });
    }
    Ok((current_temp, periods))
}

fn fetch_radar() -> Result<Vec<u8>, String> {
    let bytes = make_client()?
        .get(RADAR_URL)
        .send().map_err(|e| e.to_string())?
        .bytes().map_err(|e| e.to_string())?;
    Ok(bytes.to_vec())
}

// ── Background refresh loop ────────────────────────────────────────────────

fn refresh_loop(shared: Arc<Mutex<AppData>>) {
    loop {
        do_fetch(&shared);
        thread::sleep(Duration::from_secs(REFRESH_SECS));
    }
}

// ── Entry point ────────────────────────────────────────────────────────────

fn main() {
    let shared = Arc::new(Mutex::new(AppData {
        status: "Starting…".into(),
        ..Default::default()
    }));

    let shared_bg = Arc::clone(&shared);
    thread::spawn(move || refresh_loop(shared_bg));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Weather Radar — NWS + NOAA")
            .with_inner_size([1200.0, 760.0]),
        ..Default::default()
    };

    eframe::run_native(
        "weather-radar",
        options,
        Box::new(move |cc| {
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(WeatherApp::new(cc, Arc::clone(&shared))))
        }),
    )
    .unwrap();
}
