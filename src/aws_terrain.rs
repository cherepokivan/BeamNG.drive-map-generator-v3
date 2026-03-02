use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use image::GenericImageView;
use reqwest::Client;
use tokio::fs;

use crate::model::GenerateMapRequest;

const TILE_ZOOM: u32 = 12;
const TILE_SIZE: f64 = 256.0;

pub async fn download_heightmap(request: &GenerateMapRequest, out_dir: &Path) -> Result<PathBuf> {
    let path = out_dir.join("heightmap.raw");
    let side = request.texture_resolution as usize;

    let x_min = lon_to_tile_x(request.west, TILE_ZOOM).floor() as i32;
    let x_max = lon_to_tile_x(request.east, TILE_ZOOM).ceil() as i32;
    let y_min = lat_to_tile_y(request.north, TILE_ZOOM).floor() as i32;
    let y_max = lat_to_tile_y(request.south, TILE_ZOOM).ceil() as i32;

    let client = Client::new();
    let mut cache: HashMap<(i32, i32), image::DynamicImage> = HashMap::new();

    for tx in x_min..=x_max {
        for ty in y_min..=y_max {
            let img = download_terrarium_tile(&client, tx, ty)
                .await
                .with_context(|| {
                    format!("failed to load AWS terrarium tile z{TILE_ZOOM}/{tx}/{ty}")
                })?;
            cache.insert((tx, ty), img);
        }
    }

    let mut heights = Vec::with_capacity(side * side);
    let mut min_h = f32::INFINITY;
    let mut max_h = f32::NEG_INFINITY;

    for y in 0..side {
        let lat = lerp(
            request.north,
            request.south,
            y as f64 / (side.saturating_sub(1).max(1) as f64),
        );
        for x in 0..side {
            let lon = lerp(
                request.west,
                request.east,
                x as f64 / (side.saturating_sub(1).max(1) as f64),
            );
            let h = sample_terrarium_height(lat, lon, &cache);
            min_h = min_h.min(h);
            max_h = max_h.max(h);
            heights.push(h);
        }
    }

    let range = (max_h - min_h).max(1.0);
    let mut bytes = Vec::with_capacity(side * side * 2);
    for h in heights {
        let n = ((h - min_h) / range).clamp(0.0, 1.0);
        let value = (n * 65535.0) as u16;
        bytes.extend_from_slice(&value.to_le_bytes());
    }

    fs::write(&path, bytes).await?;
    Ok(path)
}

async fn download_terrarium_tile(client: &Client, x: i32, y: i32) -> Result<image::DynamicImage> {
    let url = format!(
        "https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{}/{}/{}.png",
        TILE_ZOOM, x, y
    );
    let bytes = client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await?;
    Ok(image::load_from_memory(&bytes)?)
}

fn sample_terrarium_height(
    lat: f64,
    lon: f64,
    cache: &HashMap<(i32, i32), image::DynamicImage>,
) -> f32 {
    let x = lon_to_tile_x(lon, TILE_ZOOM);
    let y = lat_to_tile_y(lat, TILE_ZOOM);

    let tx = x.floor() as i32;
    let ty = y.floor() as i32;

    let px = ((x - tx as f64) * TILE_SIZE).clamp(0.0, 255.0) as u32;
    let py = ((y - ty as f64) * TILE_SIZE).clamp(0.0, 255.0) as u32;

    let Some(tile) = cache.get(&(tx, ty)) else {
        return 0.0;
    };
    let rgb = tile.get_pixel(px, py);
    terrarium_to_meters(rgb[0], rgb[1], rgb[2])
}

fn terrarium_to_meters(r: u8, g: u8, b: u8) -> f32 {
    (r as f32 * 256.0 + g as f32 + b as f32 / 256.0) - 32768.0
}

fn lon_to_tile_x(lon: f64, zoom: u32) -> f64 {
    let n = 2_f64.powi(zoom as i32);
    (lon + 180.0) / 360.0 * n
}

fn lat_to_tile_y(lat: f64, zoom: u32) -> f64 {
    let lat_rad = lat.to_radians();
    let n = 2_f64.powi(zoom as i32);
    (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}
