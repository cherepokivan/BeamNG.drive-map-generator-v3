mod aws_terrain;
mod beamng_export;
mod model;
mod osm;
mod texture;

use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use anyhow::Context;
use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use model::{GenerateMapRequest, GenerateMapResponse};
use tokio::fs;
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::{error, info};

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

    let app = Router::new()
        .route("/api/generate", post(generate_map))
        .with_state(state)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    info!("BeamNG map generator API running on {addr}");
    axum::serve(tokio::net::TcpListener::bind(addr).await?, app).await?;

    Ok(())
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
