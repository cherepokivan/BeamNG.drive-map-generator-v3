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
    extract::State,
    http::StatusCode,
    routing::{get_service, post},
    Json, Router,
};
use model::{GenerateMapRequest, GenerateMapResponse};
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
    run_pipeline(state, request).await.map(Json).map_err(|err| {
        error!("map generation error: {err:#}");
        (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
    })
}

async fn run_pipeline(
    state: AppState,
    request: GenerateMapRequest,
) -> anyhow::Result<GenerateMapResponse> {
    request.validate()?;

    let build_root = state.workdir.join(format!(
        "{}_{}",
        request.map_name,
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    ));
    fs::create_dir_all(&build_root).await?;

    let osm_data = osm::download_osm_geojson(&request)
        .await
        .context("OSM data download failed")?;

    let heightmap_path = aws_terrain::download_heightmap(&request, &build_root)
        .await
        .context("AWS terrain download failed")?;

    let texture_path = texture::generate_textures(&osm_data, &build_root)
        .await
        .context("texture generation failed")?;

    let road_nodes = osm::extract_road_nodes(&osm_data)?;

    let mod_path = beamng_export::write_mod_archive(
        &request,
        &build_root,
        &heightmap_path,
        &texture_path,
        &road_nodes,
    )
    .await
    .context("BeamNG mod export failed")?;

    Ok(GenerateMapResponse {
        map_name: request.map_name,
        mod_archive: mod_path.to_string_lossy().to_string(),
        road_nodes_count: road_nodes.len(),
    })
}
