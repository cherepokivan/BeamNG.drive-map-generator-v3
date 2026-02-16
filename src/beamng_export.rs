use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;
use tokio::fs;
use zip::write::SimpleFileOptions;

use crate::model::{GenerateMapRequest, RoadNode};

pub async fn write_mod_archive(
    request: &GenerateMapRequest,
    build_root: &Path,
    heightmap_path: &Path,
    texture_path: &Path,
    road_nodes: &[RoadNode],
) -> Result<PathBuf> {
    let mod_root = build_root.join("levels").join(&request.map_name);
    fs::create_dir_all(&mod_root).await?;

    let roads_json_path = mod_root.join("road_nodes.json");
    let roads_json = serde_json::to_vec_pretty(road_nodes)?;
    fs::write(&roads_json_path, roads_json).await?;

    fs::copy(heightmap_path, mod_root.join("heightmap.raw")).await?;
    fs::copy(texture_path, mod_root.join("albedo.png")).await?;

    let info = serde_json::json!({
      "name": request.map_name,
      "generatedBy": "beamng-map-generator",
      "description": "Generated from OSM + AWS terrain workflow"
    });
    fs::write(
        mod_root.join("info.json"),
        serde_json::to_vec_pretty(&info)?,
    )
    .await?;

    let downloads = dirs::download_dir().unwrap_or_else(|| build_root.to_path_buf());
    fs::create_dir_all(&downloads).await?;
    let zip_path = downloads.join(format!("{}_beamng_mod.zip", request.map_name));

    let file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    add_file_to_zip(
        &mut zip,
        &mod_root.join("road_nodes.json"),
        "levels/{}/road_nodes.json",
        &request.map_name,
        options,
    )?;
    add_file_to_zip(
        &mut zip,
        &mod_root.join("heightmap.raw"),
        "levels/{}/heightmap.raw",
        &request.map_name,
        options,
    )?;
    add_file_to_zip(
        &mut zip,
        &mod_root.join("albedo.png"),
        "levels/{}/albedo.png",
        &request.map_name,
        options,
    )?;
    add_file_to_zip(
        &mut zip,
        &mod_root.join("info.json"),
        "levels/{}/info.json",
        &request.map_name,
        options,
    )?;

    zip.finish()?;

    Ok(zip_path)
}

fn add_file_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    src_path: &Path,
    zip_path_tmpl: &str,
    map_name: &str,
    options: SimpleFileOptions,
) -> Result<()> {
    let path = zip_path_tmpl.replacen("{}", map_name, 1);
    zip.start_file(path, options)?;
    let bytes = std::fs::read(src_path)?;
    zip.write_all(&bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn creates_zip() {
        let temp = tempfile::tempdir().unwrap();
        let h = temp.path().join("h.raw");
        let t = temp.path().join("a.png");
        tokio::fs::write(&h, [0_u8; 8]).await.unwrap();
        tokio::fs::write(&t, [1_u8; 8]).await.unwrap();

        let req = GenerateMapRequest {
            map_name: "test".into(),
            north: 1.0,
            south: 0.0,
            east: 1.0,
            west: 0.0,
            texture_resolution: 256,
        };

        let out = write_mod_archive(&req, temp.path(), &h, &t, &[])
            .await
            .unwrap();
        assert!(out.exists());
    }
}
