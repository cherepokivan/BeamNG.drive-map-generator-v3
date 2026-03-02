#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod aws_terrain;
mod beamng_export;
mod model;
mod osm;
mod texture;

use std::{
    io::ErrorKind,
    net::{IpAddr, Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::Arc,
};

use anyhow::Context;
use axum::{
    extract::{Multipart, State},
    http::StatusCode,
    routing::{get_service, post},
    Json, Router,
};
use model::{GenerateMapRequest, GenerateMapResponse};
use osm::{extract_buildings, extract_forest_areas, extract_roads};
use tokio::{fs, net::TcpListener};
use tower_http::{
    cors::CorsLayer,
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};
use tracing::{error, info, warn};

#[derive(Clone)]
struct AppState {
    workdir: Arc<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "beamng_map_generator=info,tower_http=info".into()),
        )
        .init();

    let workdir = std::env::current_dir()?.join("generated");
    fs::create_dir_all(&workdir).await?;

    let state = AppState {
        workdir: Arc::new(workdir),
    };

    let preferred_port = std::env::var("BEAMNG_MAP_GENERATOR_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8080);

    let static_dir = detect_static_dir()?;
    let index_path = static_dir.join("index.html");

    let app = Router::new()
        .route("/api/generate", post(generate_map))
        .route("/api/generate-from-osm", post(generate_map_from_osm_file))
        .fallback_service(get_service(
            ServeDir::new(&static_dir).not_found_service(ServeFile::new(index_path)),
        ))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let (listener, addr) = bind_with_fallback(preferred_port).await?;
    let url = format!("http://{addr}");

    info!("BeamNG map generator running at {url}");
    if let Err(err) = webbrowser::open(&url) {
        warn!("Could not open browser automatically: {err}");
    }

    axum::serve(listener, app).await?;

    Ok(())
}

fn detect_static_dir() -> anyhow::Result<PathBuf> {
    let local = std::env::current_dir()?.join("ui").join("dist");
    if local.exists() {
        return Ok(local);
    }

    let exe_dir = std::env::current_exe()?
        .parent()
        .map(ToOwned::to_owned)
        .context("failed to detect executable directory")?;
    let portable = exe_dir.join("ui").join("dist");
    if portable.exists() {
        return Ok(portable);
    }

    anyhow::bail!(
        "UI assets not found. Expected ./ui/dist next to project or executable. Run `cd ui && npm install && npm run build`."
    )
}

async fn bind_with_fallback(preferred_port: u16) -> anyhow::Result<(TcpListener, SocketAddr)> {
    let host = IpAddr::V4(Ipv4Addr::LOCALHOST);
    let primary = SocketAddr::new(host, preferred_port);

    match TcpListener::bind(primary).await {
        Ok(listener) => return Ok((listener, primary)),
        Err(err) if err.kind() == ErrorKind::AddrInUse => {
            warn!("Port {preferred_port} is already in use. Trying fallback ports...");
        }
        Err(err) => {
            return Err(err).with_context(|| format!("failed to bind {primary}"));
        }
    }

    for offset in 1..=20 {
        let port = preferred_port.saturating_add(offset);
        if port == preferred_port {
            continue;
        }

        let addr = SocketAddr::new(host, port);
        match TcpListener::bind(addr).await {
            Ok(listener) => {
                warn!("Using fallback port {port} because {preferred_port} is occupied");
                return Ok((listener, addr));
            }
            Err(err) if err.kind() == ErrorKind::AddrInUse => continue,
            Err(err) => {
                return Err(err).with_context(|| format!("failed to bind fallback address {addr}"));
            }
        }
    }

    anyhow::bail!(
        "could not bind API server: port {preferred_port} and fallback range {start}-{end} are busy",
        start = preferred_port.saturating_add(1),
        end = preferred_port.saturating_add(20)
    )
}

async fn generate_map(
    State(state): State<AppState>,
    Json(request): Json<GenerateMapRequest>,
) -> Result<Json<GenerateMapResponse>, (StatusCode, String)> {
    request.validate().map_err(internal)?;

    let osm_data = osm::download_osm_geojson(&request)
        .await
        .context("OSM data download failed")
        .map_err(internal)?;

    run_pipeline_with_geojson(state, request, osm_data)
        .await
        .map(Json)
        .map_err(internal)
}

async fn generate_map_from_osm_file(
    State(state): State<AppState>,
    mut multipart: Multipart,
) -> Result<Json<GenerateMapResponse>, (StatusCode, String)> {
    let mut map_name: Option<String> = None;
    let mut texture_resolution: Option<u32> = None;
    let mut osm_bytes: Option<Vec<u8>> = None;

    while let Some(field) = multipart.next_field().await.map_err(internal)? {
        let name = field.name().unwrap_or_default().to_owned();
        match name.as_str() {
            "map_name" => {
                map_name = Some(field.text().await.map_err(internal)?);
            }
            "texture_resolution" => {
                let text = field.text().await.map_err(internal)?;
                texture_resolution = text.parse::<u32>().ok();
            }
            "osm_file" => {
                osm_bytes = Some(field.bytes().await.map_err(internal)?.to_vec());
            }
            _ => {}
        }
    }

    let osm_bytes = osm_bytes.ok_or_else(|| {
        (
            StatusCode::BAD_REQUEST,
            "Missing osm_file in multipart request".to_owned(),
        )
    })?;

    let (osm_geojson, bounds) = osm::geojson_from_osm_xml(&osm_bytes)
        .context("OSM file parsing failed")
        .map_err(internal)?;

    let request = GenerateMapRequest {
        map_name: map_name.unwrap_or_else(|| "local_osm_map".to_owned()),
        north: bounds.north,
        south: bounds.south,
        east: bounds.east,
        west: bounds.west,
        texture_resolution: texture_resolution.unwrap_or(1024),
    };

    request.validate().map_err(internal)?;

    run_pipeline_with_geojson(state, request, osm_geojson)
        .await
        .map(Json)
        .map_err(internal)
}

async fn run_pipeline_with_geojson(
    state: AppState,
    request: GenerateMapRequest,
    osm_data: geojson::GeoJson,
) -> anyhow::Result<GenerateMapResponse> {
    let build_root = state.workdir.join(format!(
        "{}_{}",
        request.map_name,
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    ));
    fs::create_dir_all(&build_root).await?;

    let heightmap_path = aws_terrain::download_heightmap(&request, &build_root)
        .await
        .context("AWS terrain download failed")?;

    let texture_path = texture::generate_textures(&request, &osm_data, &build_root)
        .await
        .context("texture generation failed")?;

    let roads = extract_roads(&osm_data);
    let buildings = extract_buildings(&osm_data);
    let forests = extract_forest_areas(&osm_data);
    let road_nodes_count: usize = roads.iter().map(|r| r.points.len()).sum();

    let mod_path = beamng_export::write_mod_archive(
        &request,
        &build_root,
        &heightmap_path,
        &texture_path,
        &roads,
        &buildings,
        &forests,
    )
    .await
    .context("BeamNG mod export failed")?;

    Ok(GenerateMapResponse {
        map_name: request.map_name,
        mod_archive: mod_path.to_string_lossy().to_string(),
        road_nodes_count,
    })
}

fn internal(err: impl std::fmt::Display) -> (StatusCode, String) {
    error!("map generation error: {err}");
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
