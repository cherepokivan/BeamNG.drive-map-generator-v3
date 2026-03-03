use std::path::{Path, PathBuf};

use anyhow::Result;
use geojson::{GeoJson, Value};
use image::{ImageBuffer, Rgb};

use crate::model::GenerateMapRequest;

pub struct TextureAssets {
    pub terrain_albedo: PathBuf,
    pub terrain_normal: PathBuf,
    pub terrain_roughness: PathBuf,
    pub building_wall_albedo: PathBuf,
    pub building_wall_normal: PathBuf,
    pub building_roof_albedo: PathBuf,
    pub building_roof_normal: PathBuf,
}

pub async fn generate_textures(
    request: &GenerateMapRequest,
    geojson: &GeoJson,
    out_dir: &Path,
) -> Result<TextureAssets> {
    let size = request.texture_resolution;

    let terrain_albedo = out_dir.join("terrain_albedo.png");
    let terrain_normal = out_dir.join("terrain_normal.png");
    let terrain_roughness = out_dir.join("terrain_roughness.png");
    let building_wall_albedo = out_dir.join("building_wall_albedo.png");
    let building_wall_normal = out_dir.join("building_wall_normal.png");
    let building_roof_albedo = out_dir.join("building_roof_albedo.png");
    let building_roof_normal = out_dir.join("building_roof_normal.png");

    let mut albedo = ImageBuffer::from_pixel(size, size, Rgb([84, 114, 74]));
    let mut roughness = ImageBuffer::from_pixel(size, size, Rgb([150, 150, 150]));

    if let GeoJson::FeatureCollection(fc) = geojson {
        for feature in &fc.features {
            if let Some(geom) = &feature.geometry {
                match &geom.value {
                    Value::LineString(line) => {
                        paint_line(&mut albedo, line, [120, 120, 120], request);
                        paint_line(&mut roughness, line, [200, 200, 200], request);
                    }
                    Value::Polygon(poly) => {
                        if let Some(ring) = poly.first() {
                            paint_line(&mut albedo, ring, [110, 140, 95], request);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    let normal_flat = ImageBuffer::from_pixel(size, size, Rgb([128_u8, 128_u8, 255_u8]));
    let wall_albedo = procedural_wall_texture(512, 512);
    let wall_normal = procedural_normal_flat(512, 512);
    let roof_albedo = procedural_roof_texture(512, 512);
    let roof_normal = procedural_normal_flat(512, 512);

    albedo.save(&terrain_albedo)?;
    normal_flat.save(&terrain_normal)?;
    roughness.save(&terrain_roughness)?;
    wall_albedo.save(&building_wall_albedo)?;
    wall_normal.save(&building_wall_normal)?;
    roof_albedo.save(&building_roof_albedo)?;
    roof_normal.save(&building_roof_normal)?;

    Ok(TextureAssets {
        terrain_albedo,
        terrain_normal,
        terrain_roughness,
        building_wall_albedo,
        building_wall_normal,
        building_roof_albedo,
        building_roof_normal,
    })
}

fn procedural_wall_texture(w: u32, h: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    ImageBuffer::from_fn(w, h, |x, y| {
        let brick_x = (x / 32) % 2;
        let mortar = x % 32 == 0 || y % 16 == 0 || (y / 16) % 2 == 1 && x % 32 == 16;
        if mortar {
            Rgb([120, 120, 120])
        } else if brick_x == 0 {
            Rgb([163, 88, 72])
        } else {
            Rgb([154, 80, 66])
        }
    })
}

fn procedural_roof_texture(w: u32, h: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    ImageBuffer::from_fn(w, h, |x, y| {
        let checker = ((x / 24) + (y / 24)) % 2 == 0;
        if checker {
            Rgb([86, 84, 90])
        } else {
            Rgb([72, 70, 76])
        }
    })
}

fn procedural_normal_flat(w: u32, h: u32) -> ImageBuffer<Rgb<u8>, Vec<u8>> {
    ImageBuffer::from_pixel(w, h, Rgb([128, 128, 255]))
}

fn paint_line(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    coords: &[Vec<f64>],
    color: [u8; 3],
    request: &GenerateMapRequest,
) {
    for pair in coords.windows(2) {
        let a = geo_to_pixel(pair[0][1], pair[0][0], img.width(), img.height(), request);
        let b = geo_to_pixel(pair[1][1], pair[1][0], img.width(), img.height(), request);
        draw_bresenham(img, a, b, color);
    }
}

fn geo_to_pixel(
    lat: f64,
    lon: f64,
    width: u32,
    height: u32,
    request: &GenerateMapRequest,
) -> (i32, i32) {
    let x = ((lon - request.west) / (request.east - request.west) * width as f64) as i32;
    let y = ((request.north - lat) / (request.north - request.south) * height as f64) as i32;
    (x.clamp(0, width as i32 - 1), y.clamp(0, height as i32 - 1))
}

fn draw_bresenham(
    img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>,
    from: (i32, i32),
    to: (i32, i32),
    color: [u8; 3],
) {
    let (mut x0, mut y0) = from;
    let (x1, y1) = to;
    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        img.put_pixel(x0 as u32, y0 as u32, Rgb(color));
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}
