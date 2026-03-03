use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result};
use image::GenericImageView;
use reqwest::Client;
use tokio::fs;

use crate::model::GenerateMapRequest;

const TILE_ZOOM: u32 = 12;
const TILE_SIZE: f64 = 256.0;
const TERRARIUM_ENDPOINTS: [&str; 2] = [
    "https://s3.amazonaws.com/elevation-tiles-prod/terrarium",
    "https://elevation-tiles-prod.s3.amazonaws.com/terrarium",
];
const HTTP_TIMEOUT_SECONDS: u64 = 20;
const TILE_DOWNLOAD_RETRIES: usize = 3;

pub async fn download_heightmap(request: &GenerateMapRequest, out_dir: &Path) -> Result<PathBuf> {
    let path = out_dir.join("heightmap.raw");
    let side = request.texture_resolution as usize;

    let x_min = lon_to_tile_x(request.west, TILE_ZOOM).floor() as i32;
    let x_max = lon_to_tile_x(request.east, TILE_ZOOM).ceil() as i32;
    let y_min = lat_to_tile_y(request.north, TILE_ZOOM).floor() as i32;
    let y_max = lat_to_tile_y(request.south, TILE_ZOOM).ceil() as i32;

    let client = Client::builder()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECONDS))
        .user_agent("TerraForge/1.0")
        .build()
        .context("failed to build HTTP client for AWS Terrarium")?;
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
    let tile_x = wrap_tile_x(x, TILE_ZOOM);
    let max_y = max_tile_index(TILE_ZOOM);
    let tile_y = y.clamp(0, max_y);

    for endpoint in TERRARIUM_ENDPOINTS {
        let url = format!("{endpoint}/{}/{}/{}.png", TILE_ZOOM, tile_x, tile_y);
        for attempt in 1..=TILE_DOWNLOAD_RETRIES {
            let response = client.get(&url).send().await;
            if let Ok(response) = response {
                let response = response.error_for_status();
                if let Ok(response) = response {
                    let bytes = response.bytes().await?;
                    return Ok(image::load_from_memory(&bytes)?);
                }
            }

            if attempt < TILE_DOWNLOAD_RETRIES {
                tokio::time::sleep(Duration::from_millis((attempt as u64) * 250)).await;
            }
        }
    }

    anyhow::bail!(
        "all AWS Terrarium endpoints failed for z{TILE_ZOOM}/{tile_x}/{tile_y} after {TILE_DOWNLOAD_RETRIES} retries"
    )
}

fn sample_terrarium_height(
    lat: f64,
    lon: f64,
    cache: &HashMap<(i32, i32), image::DynamicImage>,
) -> f32 {
    let x = lon_to_tile_x(normalize_longitude(lon), TILE_ZOOM);
    let y = lat_to_tile_y(clamp_mercator_lat(lat), TILE_ZOOM);

    let tx = wrap_tile_x(x.floor() as i32, TILE_ZOOM);
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

fn normalize_longitude(mut lon: f64) -> f64 {
    while lon < -180.0 {
        lon += 360.0;
    }
    while lon >= 180.0 {
        lon -= 360.0;
    }
    lon
}

fn clamp_mercator_lat(lat: f64) -> f64 {
    lat.clamp(-85.0511, 85.0511)
}

fn max_tile_index(zoom: u32) -> i32 {
    (2_i32.pow(zoom) - 1).max(0)
}

fn wrap_tile_x(x: i32, zoom: u32) -> i32 {
    let world = 2_i32.pow(zoom);
    let mut value = x % world;
    if value < 0 {
        value += world;
    }
    value
}

#[cfg(test)]
mod tests {
    use super::{clamp_mercator_lat, normalize_longitude, terrarium_to_meters, wrap_tile_x};

    #[test]
    fn terrarium_decoding_is_correct() {
        let zero = terrarium_to_meters(128, 0, 0);
        assert!((zero - 0.0).abs() < 0.01);

        let one_meter = terrarium_to_meters(128, 1, 0);
        assert!((one_meter - 1.0).abs() < 0.01);
    }

    #[test]
    fn coordinates_are_clamped_and_wrapped() {
        assert_eq!(normalize_longitude(190.0), -170.0);
        assert_eq!(normalize_longitude(-190.0), 170.0);
        assert_eq!(wrap_tile_x(-1, 12), 4095);
        assert_eq!(wrap_tile_x(4096, 12), 0);
        assert_eq!(clamp_mercator_lat(90.0), 85.0511);
        assert_eq!(clamp_mercator_lat(-90.0), -85.0511);
    }
}
