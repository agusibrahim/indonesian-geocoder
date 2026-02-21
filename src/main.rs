use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use futures_util::StreamExt;
use geo::{Contains, Point};
use geo::prelude::HaversineDistance;
use geozero::{wkb::Wkb, ToGeo};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::{sqlite::{SqliteConnectOptions, SqlitePoolOptions}, Pool, Row, Sqlite};
use std::{env, io::Write, path::Path, sync::Arc};
use tower_http::cors::CorsLayer;
use tracing::info;

const DB_NAME: &str = "indonesia_area.db";
const DB_DOWNLOAD_URL: &str = "https://github.com/agusibrahim/indonesian-geocoder/releases/download/db/indonesia_area.db";

#[derive(Clone)]
struct AppState {
    db: Pool<Sqlite>,
}

// Request Models
#[derive(Deserialize)]
struct ReverseGeocodeQuery {
    lat: f64,
    lng: f64,
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default)]
    limit: usize,
    lat: Option<f64>,
    lng: Option<f64>,
}

// Response Models
#[derive(Serialize, Clone)]
struct LocationDetail {
    province: String,
    regency: String,
    district: String,
    village: String,
}

#[derive(Serialize, Clone)]
struct LocationInfo {
    level: String,
    id: String,
    name: String,

    // Level administratif dipisah ke dalam satu objek
    location_detail: LocationDetail,

    full_name: String,

    // Centroid coordinate
    lat: f64,
    lng: f64,

    // Jarak dari titik input (dalam meter, tipe integer)
    distance_meters: Option<i64>,
}

#[derive(Serialize)]
struct APIResponse<T> {
    success: bool,
    data: Option<T>,
    error: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Set default RUST_LOG ke info jika user tidak mensetnya
    if env::var("RUST_LOG").is_err() {
        env::set_var("RUST_LOG", "info");
    }
    tracing_subscriber::fmt::init();

    if !Path::new(DB_NAME).exists() {
        info!("Database {} not found. Downloading...", DB_NAME);
        download_database().await?;
    } else {
        info!("Database {} found.", DB_NAME);
    }

    let db_options = SqliteConnectOptions::new()
        .filename(DB_NAME)
        .read_only(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(100)
        .connect_with(db_options)
        .await?;

    let state = Arc::new(AppState { db: pool });

    let app = Router::new()
        .route("/api/v1/geocode/reverse", get(reverse_geocode))
        .route("/api/v1/places/search", get(search_places))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let port = env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let addr = format!("0.0.0.0:{}", port);

    info!("🚀 Server running on http://{}", addr);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

async fn download_database() -> anyhow::Result<()> {
    info!("Downloading database from {}...", DB_DOWNLOAD_URL);

    // Create client that follows redirects
    let client = Client::builder()
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()?;

    let response = client.get(DB_DOWNLOAD_URL).send().await?;

    if !response.status().is_success() {
        tracing::warn!("Failed to download database (Status: {}). Make sure {} exists.", response.status(), DB_NAME);
        return Ok(());
    }

    let total_size = response.content_length().unwrap_or(0);
    info!("Starting download... (Total size: {} bytes)", total_size);

    let mut file = std::fs::File::create(DB_NAME)?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    let mut last_percent = 0;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        file.write_all(&chunk)?;
        downloaded += chunk.len() as u64;

        if total_size > 0 {
            let percent = (downloaded as f64 / total_size as f64 * 100.0) as u64;
            // Hanya print setiap kelipatan 5% untuk menghindari spam log
            if percent >= last_percent + 5 || percent == 100 {
                info!("Downloading: {}%", percent);
                last_percent = percent;
            }
        }
    }

    info!("Database downloaded successfully!");
    Ok(())
}

async fn reverse_geocode(
    State(state): State<Arc<AppState>>,
    Query(params): Query<ReverseGeocodeQuery>,
) -> impl IntoResponse {
    let lat = params.lat;
    let lng = params.lng;
    let user_point = Point::new(lng, lat);

    let query = r#"
        SELECT v.id, v.name as village_name,
               d.name as district_name, r.name as regency_name, p.name as province_name,
               v.lat, v.lng, v.boundaries
        FROM villages v
        LEFT JOIN districts d ON v.parent_id = d.id
        LEFT JOIN regencies r ON d.parent_id = r.id
        LEFT JOIN provinces p ON r.parent_id = p.id
        WHERE ? BETWEEN v.min_lat AND v.max_lat
          AND ? BETWEEN v.min_lng AND v.max_lng
    "#;

    match sqlx::query(query).bind(lat).bind(lng).fetch_all(&state.db).await {
        Ok(rows) => {
            for row in rows {
                let wkb_data: Vec<u8> = row.get("boundaries");
                let wkb_geom = Wkb(wkb_data);

                if let Ok(geom) = wkb_geom.to_geo() {
                    if geom.contains(&user_point) {
                        let id: String = row.get("id");
                        let v_name: String = row.get("village_name");
                        let d_name: String = row.get("district_name");
                        let r_name: String = row.get("regency_name");
                        let p_name: String = row.get("province_name");
                        let centroid_lat: f64 = row.get("lat");
                        let centroid_lng: f64 = row.get("lng");

                        let full_name = format!("Kelurahan {}, Kecamatan {}, {}, {}", v_name, d_name, r_name, p_name);

                        let center_point = Point::new(centroid_lng, centroid_lat);
                        let distance = user_point.haversine_distance(&center_point);

                        return Json(APIResponse {
                            success: true,
                            data: Some(LocationInfo {
                                level: "village".to_string(),
                                id,
                                name: v_name.clone(),
                                location_detail: LocationDetail {
                                    province: p_name,
                                    regency: r_name,
                                    district: d_name,
                                    village: v_name,
                                },
                                full_name,
                                lat: centroid_lat,
                                lng: centroid_lng,
                                distance_meters: Some(distance.round() as i64),
                            }),
                            error: None,
                        }).into_response();
                    }
                }
            }

            Json(APIResponse::<LocationInfo> {
                success: false, data: None,
                error: Some("Location not found".to_string()),
            }).into_response()
        },
        Err(e) => {
            tracing::error!("Database error: {}", e);
            (StatusCode::INTERNAL_SERVER_ERROR, Json(APIResponse::<LocationInfo> {
                success: false, data: None, error: Some("Internal server error".to_string()),
            })).into_response()
        }
    }
}

async fn search_places(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchQuery>,
) -> impl IntoResponse {
    let limit = if params.limit == 0 { 10 } else { params.limit.min(50) };

    let keywords: Vec<String> = params.q
        .to_lowercase()
        .split_whitespace()
        .map(|s| format!("%{}%", s))
        .collect();

    if keywords.is_empty() {
        return Json(APIResponse {
            success: true,
            data: Some(Vec::<LocationInfo>::new()),
            error: None,
        }).into_response();
    }

    let mut where_clauses = Vec::new();
    for _ in 0..keywords.len() {
        where_clauses.push("(LOWER(v.name) LIKE ? OR LOWER(d.name) LIKE ? OR LOWER(r.name) LIKE ? OR LOWER(p.name) LIKE ?)");
    }

    let where_sql = where_clauses.join(" AND ");

    let query_str = format!(
        r#"
        SELECT 'village' as level, v.id, v.name as v_name, d.name as d_name, r.name as r_name, p.name as p_name, v.lat, v.lng
        FROM villages v
        LEFT JOIN districts d ON v.parent_id = d.id
        LEFT JOIN regencies r ON d.parent_id = r.id
        LEFT JOIN provinces p ON r.parent_id = p.id
        WHERE {}
        LIMIT 100
        "#,
        where_sql
    );

    let mut query_builder = sqlx::query(&query_str);

    for kw in &keywords {
        query_builder = query_builder.bind(kw).bind(kw).bind(kw).bind(kw);
    }

    match query_builder.fetch_all(&state.db).await {
        Ok(rows) => {
            let mut results = Vec::new();

            let user_loc = match (params.lat, params.lng) {
                (Some(lat), Some(lng)) => Some(Point::new(lng, lat)),
                _ => None,
            };

            for row in rows {
                let v_name: String = row.get("v_name");
                let d_name: String = row.get("d_name");
                let r_name: String = row.get("r_name");
                let p_name: String = row.get("p_name");
                let centroid_lat: f64 = row.get("lat");
                let centroid_lng: f64 = row.get("lng");

                let full_name = format!("Kelurahan {}, Kecamatan {}, {}, {}", v_name, d_name, r_name, p_name);

                let mut dist_meters = None;
                if let Some(user_pt) = user_loc {
                    let loc_pt = Point::new(centroid_lng, centroid_lat);
                    dist_meters = Some(user_pt.haversine_distance(&loc_pt).round() as i64);
                }

                results.push(LocationInfo {
                    level: row.get("level"),
                    id: row.get("id"),
                    name: v_name.clone(),
                    location_detail: LocationDetail {
                        province: p_name,
                        regency: r_name,
                        district: d_name,
                        village: v_name,
                    },
                    full_name,
                    lat: centroid_lat,
                    lng: centroid_lng,
                    distance_meters: dist_meters,
                });
            }

            if user_loc.is_some() {
                results.sort_by(|a, b| {
                    a.distance_meters.unwrap_or(i64::MAX)
                     .cmp(&b.distance_meters.unwrap_or(i64::MAX))
                });
            } else {
                results.sort_by(|a, b| a.name.len().cmp(&b.name.len()));
            }

            results.truncate(limit);

            Json(APIResponse {
                success: true,
                data: Some(results),
                error: None,
            }).into_response()
        },
        Err(e) => {
            tracing::error!("Database error: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(APIResponse::<Vec<LocationInfo>> {
                    success: false,
                    data: None,
                    error: Some("Internal server error".to_string()),
                })
            ).into_response()
        }
    }
}
