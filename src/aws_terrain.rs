use std::path::{Path, PathBuf};

use anyhow::Result;
use tokio::fs;

use crate::model::GenerateMapRequest;

/// Downloads (or synthesizes) a heightmap using AWS-hosted Terrain RGB tiles.
///
/// In production you can replace this with an AWS SDK pipeline that fetches
/// DEM tiles from your own S3 bucket / Terrain service.
pub async fn download_heightmap(request: &GenerateMapRequest, out_dir: &Path) -> Result<PathBuf> {
    let path = out_dir.join("heightmap.raw");
    let side = request.texture_resolution as usize;

    // Placeholder gradient-based heightfield to keep pipeline deterministic.
    let mut bytes = Vec::with_capacity(side * side * 2);
    for y in 0..side {
        for x in 0..side {
            let value: u16 = (((x + y) as f32 / (2.0 * side as f32)) * 65535.0) as u16;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }

    fs::write(&path, bytes).await?;
    Ok(path)
}
