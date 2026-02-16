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
        if !(256..=8192).contains(&self.texture_resolution) {
            bail!("texture_resolution must be in 256..=8192");
        }

        Ok(())
    }

    pub fn bbox_str(&self) -> String {
        format!("{},{},{},{}", self.south, self.west, self.north, self.east)
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
}
