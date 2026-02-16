use std::path::{Path, PathBuf};

use anyhow::Result;
use geojson::{GeoJson, Value};
use image::{ImageBuffer, Rgb};

pub async fn generate_textures(geojson: &GeoJson, out_dir: &Path) -> Result<PathBuf> {
    let path = out_dir.join("albedo.png");
    let mut image = ImageBuffer::from_pixel(1024, 1024, Rgb([74, 110, 61]));

    if let GeoJson::FeatureCollection(fc) = geojson {
        for feature in &fc.features {
            if let Some(geom) = &feature.geometry {
                match &geom.value {
                    Value::LineString(line) => paint_line(&mut image, line, [128, 128, 128]),
                    Value::Polygon(poly) => {
                        if let Some(ring) = poly.first() {
                            paint_line(&mut image, ring, [98, 142, 84]);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    image.save(&path)?;
    Ok(path)
}

fn paint_line(img: &mut ImageBuffer<Rgb<u8>, Vec<u8>>, coords: &[Vec<f64>], color: [u8; 3]) {
    for pair in coords.windows(2) {
        let a = geo_to_pixel(pair[0][1], pair[0][0], img.width(), img.height());
        let b = geo_to_pixel(pair[1][1], pair[1][0], img.width(), img.height());
        draw_bresenham(img, a, b, color);
    }
}

fn geo_to_pixel(lat: f64, lon: f64, width: u32, height: u32) -> (i32, i32) {
    let x = (((lon + 180.0) / 360.0) * width as f64) as i32;
    let y = (((90.0 - lat) / 180.0) * height as f64) as i32;
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
