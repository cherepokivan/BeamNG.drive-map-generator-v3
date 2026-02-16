use anyhow::{anyhow, Result};
use geojson::{Feature, GeoJson, Value};
use reqwest::Client;
use serde_json::json;

use crate::model::{GenerateMapRequest, RoadNode};

pub async fn download_osm_geojson(request: &GenerateMapRequest) -> Result<GeoJson> {
    let query = format!(
        r#"[out:json][timeout:180];
        (
          way["highway"]({bbox});
          relation["building"]({bbox});
          way["landuse"]({bbox});
        );
        out geom;"#,
        bbox = request.bbox_str()
    );

    let response = Client::new()
        .post("https://overpass-api.de/api/interpreter")
        .body(query)
        .send()
        .await?
        .error_for_status()?;

    let raw: serde_json::Value = response.json().await?;
    let geojson = overpass_to_geojson(&raw)?;
    Ok(geojson)
}

fn overpass_to_geojson(value: &serde_json::Value) -> Result<GeoJson> {
    let mut features = Vec::new();
    let elements = value
        .get("elements")
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow!("no elements in overpass response"))?;

    for element in elements {
        let tags = element.get("tags").cloned().unwrap_or_else(|| json!({}));
        let geometry = element
            .get("geometry")
            .and_then(|g| g.as_array())
            .ok_or_else(|| anyhow!("missing geometry"))?;

        let coords: Vec<Vec<f64>> = geometry
            .iter()
            .filter_map(|p| Some(vec![p.get("lon")?.as_f64()?, p.get("lat")?.as_f64()?]))
            .collect();

        if coords.len() < 2 {
            continue;
        }

        let is_closed = coords.first() == coords.last();
        let geom = if is_closed && coords.len() > 3 {
            geojson::Geometry::new(Value::Polygon(vec![coords]))
        } else {
            geojson::Geometry::new(Value::LineString(coords))
        };

        features.push(Feature {
            geometry: Some(geom),
            properties: Some(tags.as_object().cloned().unwrap_or_default()),
            id: None,
            bbox: None,
            foreign_members: None,
        });
    }

    Ok(GeoJson::FeatureCollection(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    }))
}

pub fn extract_road_nodes(geojson: &GeoJson) -> Result<Vec<RoadNode>> {
    let mut nodes = Vec::new();
    let fc = match geojson {
        GeoJson::FeatureCollection(fc) => fc,
        _ => return Ok(nodes),
    };

    for feature in &fc.features {
        let props = feature.properties.as_ref();
        let has_highway = props
            .and_then(|p| p.get("highway"))
            .map(|v| !v.is_null())
            .unwrap_or(false);

        if !has_highway {
            continue;
        }

        if let Some(geom) = &feature.geometry {
            if let Value::LineString(points) = &geom.value {
                for point in points {
                    if let [lon, lat] = point.as_slice() {
                        nodes.push(RoadNode {
                            lat: *lat,
                            lon: *lon,
                        });
                    }
                }
            }
        }
    }

    Ok(nodes)
}
