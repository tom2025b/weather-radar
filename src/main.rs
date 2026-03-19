/// weather-radar — NWS 7-day forecast + live NOAA radar GUI
/// Location: Alachua, FL (29.80997, -82.4675)
/// Data: api.weather.gov (forecast) + opengeo.ncep.noaa.gov WMS (radar)
/// License: Unlicense (public domain)
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui;

// ── Constants ──────────────────────────────────────────────────────────────

const LAT: f64 = 29.80997;
const LON: f64 = -82.4675;
const REFRESH_SECS: u64 = 300;
const USER_AGENT: &str = "weather-radar/0.1 (thomaslane2025@gmail.com)";

/// NOAA WMS base-reflectivity composite — FL/SE region
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
    name:          String,
    temperature:   i64,
    unit:          String,
    short_forecast: String,
    detailed_forecast: String,
    wind_speed:    String,
    wind_direction: String,
    is_daytime:    bool,
}

#[derive(Default)]
struct AppData {
    periods:       Vec<ForecastPeriod>,
    current_temp:  Option<f64>,
    radar_bytes:   Option<Vec<u8>>,
    radar_dirty:   bool,   // true = new image waiting to be uploaded to GPU
    status:        String,
    last_updated:  Option<Instant>,
    detail_open:   Option<usize>, // index of period with expanded detail
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

// ── eframe::App ────────────────────────────────────────────────────────────

impl eframe::App for WeatherApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Repaint periodically so elapsed-time display stays live
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
                            [width, height],
                            &rgba,
                        );
                        self.radar_texture = Some(ctx.load_texture(
                            "radar",
                            color_img,
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                data.radar_dirty = false;
            }
        }

        // Snapshot shared state for this frame
        let (periods, current_temp, status, last_updated, detail_open) = {
            let data = self.shared.lock().unwrap();
            (
                data.periods.clone(),
                data.current_temp,
                data.status.clone(),
                data.last_updated,
                data.detail_open,
            )
        };

        // ── Header bar ─────────────────────────────────────────────────────
        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(10, 30, 60)).inner_margin(egui::Margin::same(8.0)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("⛅  Alachua, FL  —  NWS + Live Radar")
                            .size(18.0).strong().color(egui::Color32::WHITE),
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
                            let txt = if s < 60 {
                                format!("Updated {}s ago", s)
                            } else {
                                format!("Updated {}m ago", s / 60)
                            };
                            ui.label(egui::RichText::new(txt).small().color(egui::Color32::LIGHT_GRAY));
                        } else {
                            ui.label(egui::RichText::new("Fetching…").small().color(egui::Color32::YELLOW));
                        }
                    });
                });
            });

        // ── Status bar ─────────────────────────────────────────────────────
        egui::TopBottomPanel::bottom("status")
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(5, 15, 35)).inner_margin(egui::Margin::same(4.0)))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(format!("● {}", status))
                        .small().color(egui::Color32::from_rgb(120, 180, 120)),
                );
            });

        // ── Radar panel (right) ────────────────────────────────────────────
        egui::SidePanel::right("radar_panel")
            .min_width(440.0)
            .max_width(700.0)
            .resizable(true)
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(5, 10, 25)).inner_margin(egui::Margin::same(8.0)))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("📡  NOAA Base Reflectivity — FL/SE")
                        .strong().color(egui::Color32::WHITE),
                );
                ui.separator();

                if let Some(tex) = &self.radar_texture {
                    let avail = ui.available_size();
                    let img_w = avail.x;
                    let img_h = img_w * (427.0 / 640.0); // preserve aspect ratio
                    ui.image((tex.id(), egui::vec2(img_w, img_h)));

                    // Crosshair marker for Alachua position
                    // (approximate pixel position within the image)
                    let rect = ui.min_rect(); // last widget rect
                    let _ = rect; // used for future enhancement
                } else {
                    ui.vertical_centered(|ui| {
                        ui.add_space(80.0);
                        ui.label(egui::RichText::new("⏳  Loading radar image…").color(egui::Color32::YELLOW));
                        ui.add_space(4.0);
                        ui.label(egui::RichText::new("(NOAA WMS — may take a few seconds)").small().color(egui::Color32::GRAY));
                    });
                }

                // Refresh button
                ui.add_space(8.0);
                if ui.button("🔄  Refresh now").clicked() {
                    let shared = Arc::clone(&self.shared);
                    thread::spawn(move || {
                        do_fetch(&shared);
                    });
                }

                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Data: api.weather.gov + opengeo.ncep.noaa.gov")
                        .small().color(egui::Color32::DARK_GRAY),
                );
            });

        // ── Forecast panel (center) ────────────────────────────────────────
        egui::CentralPanel::default()
            .frame(egui::Frame::default().fill(egui::Color32::from_rgb(8, 18, 40)))
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("📋  NWS 7-Day Forecast")
                        .strong().color(egui::Color32::WHITE),
                );
                ui.separator();

                if periods.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(60.0);
                        ui.label(egui::RichText::new("Fetching forecast from api.weather.gov…")
                            .color(egui::Color32::YELLOW));
                    });
                    return;
                }

                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (i, period) in periods.iter().enumerate() {
                        let is_open = detail_open == Some(i);
                        let bg = if period.is_daytime {
                            egui::Color32::from_rgb(20, 50, 100)
                        } else {
                            egui::Color32::from_rgb(10, 25, 55)
                        };

                        let frame = egui::Frame::default()
                            .fill(bg)
                            .rounding(egui::Rounding::same(8.0))
                            .inner_margin(egui::Margin::same(10.0))
                            .outer_margin(egui::Margin { bottom: 6.0, ..Default::default() });

                        frame.show(ui, |ui| {
                            // Row: name + temp
                            ui.horizontal(|ui| {
                                let icon = if period.is_daytime { "☀️" } else { "🌙" };
                                ui.label(
                                    egui::RichText::new(format!("{} {}", icon, period.name))
                                        .strong().size(14.0).color(egui::Color32::WHITE),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}°{}", period.temperature, period.unit))
                                            .strong().size(16.0).color(temp_color(period.temperature)),
                                    );
                                });
                            });

                            // Short forecast
                            ui.label(
                                egui::RichText::new(&period.short_forecast)
                                    .color(egui::Color32::LIGHT_GRAY),
                            );

                            // Wind
                            ui.label(
                                egui::RichText::new(
                                    format!("💨 {} {}", period.wind_direction, period.wind_speed)
                                ).small().color(egui::Color32::from_rgb(160, 200, 255)),
                            );

                            // Expand/collapse detailed forecast
                            ui.add_space(2.0);
                            let btn_txt = if is_open { "▲ less" } else { "▼ details" };
                            if ui.small_button(btn_txt).clicked() {
                                let mut data = self.shared.lock().unwrap();
                                data.detail_open = if is_open { None } else { Some(i) };
                            }

                            if is_open && !period.detailed_forecast.is_empty() {
                                ui.add_space(4.0);
                                egui::Frame::default()
                                    .fill(egui::Color32::from_rgb(5, 15, 35))
                                    .rounding(egui::Rounding::same(4.0))
                                    .inner_margin(egui::Margin::same(8.0))
                                    .show(ui, |ui| {
                                        ui.label(
                                            egui::RichText::new(&period.detailed_forecast)
                                                .small().color(egui::Color32::LIGHT_GRAY),
                                        );
                                    });
                            }
                        });
                    }
                });
            });
    }
}

// ── Color helpers ──────────────────────────────────────────────────────────

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

// ── Data fetching ──────────────────────────────────────────────────────────

fn do_fetch(shared: &Arc<Mutex<AppData>>) {
    set_status(shared, "Fetching NWS forecast…".into());

    match fetch_forecast() {
        Ok((temp, periods)) => {
            let mut d = shared.lock().unwrap();
            d.current_temp = Some(temp);
            d.periods = periods;
            d.status = "Forecast OK — fetching radar…".into();
        }
        Err(e) => set_status(shared, format!("Forecast error: {e}")),
    }

    set_status(shared, "Fetching NOAA radar image…".into());

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

fn fetch_forecast() -> Result<(f64, Vec<ForecastPeriod>), String> {
    let client = make_client()?;

    // Step 1: resolve NWS grid point for our coordinates
    let points_url = format!("https://api.weather.gov/points/{},{}", LAT, LON);
    let points: serde_json::Value = client
        .get(&points_url)
        .send().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let forecast_url = points["properties"]["forecast"]
        .as_str()
        .ok_or("NWS: missing forecast URL in points response")?
        .to_string();

    // Step 2: fetch the forecast periods
    let fc: serde_json::Value = client
        .get(&forecast_url)
        .send().map_err(|e| e.to_string())?
        .json().map_err(|e| e.to_string())?;

    let raw = fc["properties"]["periods"]
        .as_array()
        .ok_or("NWS: missing periods array")?;

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
        });
    }

    Ok((current_temp, periods))
}

fn fetch_radar() -> Result<Vec<u8>, String> {
    let client = make_client()?;
    let bytes = client
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

    // Background thread: fetch and refresh forever
    let shared_bg = Arc::clone(&shared);
    thread::spawn(move || refresh_loop(shared_bg));

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Weather Radar — Alachua, FL")
            .with_inner_size([1200.0, 740.0]),
        ..Default::default()
    };

    eframe::run_native(
        "weather-radar",
        options,
        Box::new(move |cc| {
            // Dark visuals
            cc.egui_ctx.set_visuals(egui::Visuals::dark());
            Ok(Box::new(WeatherApp::new(cc, Arc::clone(&shared))))
        }),
    )
    .unwrap();
}
