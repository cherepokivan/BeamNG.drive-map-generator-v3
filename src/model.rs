use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
pub struct GenerateMapRequest {
    pub map_name: String,
    pub north: f64,
    pub south: f64,
    pub east: f64,
    pub west: f64,
    pub texture_resolution: u32,
}

impl GenerateMapRequest {
    pub fn validate(&self) -> Result<()> {
        if self.map_name.trim().is_empty() {
            bail!("map_name must not be empty");
        }
        if self.south >= self.north {
            bail!("south must be lower than north");
        }
        if self.west >= self.east {
            bail!("west must be lower than east");
        }
        if !(512..=8192).contains(&self.texture_resolution) {
            bail!("texture_resolution must be in 512..=8192");
        }

        Ok(())
    }

    pub fn bbox_str(&self) -> String {
        format!("{},{},{},{}", self.south, self.west, self.north, self.east)
    }
}

pub fn sanitize_map_name(raw: &str) -> String {
    let trimmed = raw.trim();
    let mapped: String = trimmed
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let collapsed = mapped
        .split('_')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("_");
    let safe = collapsed.trim_matches('_').trim_matches('-');

    if safe.is_empty() {
        "generated_map".to_owned()
    } else {
        safe.chars().take(48).collect()
    }
}

#[derive(Debug, Serialize)]
pub struct GenerateMapResponse {
    pub map_name: String,
    pub mod_archive: String,
    pub road_nodes_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoadNode {
    pub lat: f64,
    pub lon: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_bbox() {
        let req = GenerateMapRequest {
            map_name: "demo".to_owned(),
            north: 55.8,
            south: 55.7,
            east: 37.7,
            west: 37.5,
            texture_resolution: 1024,
        };
        assert!(req.validate().is_ok());
    }

    #[test]
    fn sanitizes_map_name() {
        assert_eq!(sanitize_map_name("  my/city..map  "), "my_city_map");
        assert_eq!(sanitize_map_name("***"), "generated_map");
    }
}
