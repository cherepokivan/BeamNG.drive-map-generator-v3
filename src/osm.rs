use std::collections::HashMap;

use anyhow::{anyhow, bail, Context, Result};
use geojson::{Feature, GeoJson, Value};
use quick_xml::{events::Event, Reader};
use reqwest::Client;
use serde_json::{json, Map as JsonMap, Value as JsonValue};

use crate::model::{GenerateMapRequest, RoadNode};

#[derive(Debug, Clone, Copy)]
pub struct OsmBounds {
    pub north: f64,
    pub south: f64,
    pub east: f64,
    pub west: f64,
}

pub async fn download_osm_geojson(request: &GenerateMapRequest) -> Result<GeoJson> {
    let query = format!(
        r#"[out:json][timeout:180];
        (
          way["highway"]({bbox});
          way["building"]({bbox});
          way["landuse"]({bbox});
          relation["building"]({bbox});
        );
        out geom;"#,
        bbox = request.bbox_str()
    );

    let endpoints = [
        "https://overpass-api.de/api/interpreter",
        "https://overpass.kumi.systems/api/interpreter",
        "https://overpass.openstreetmap.ru/api/interpreter",
    ];

    let client = Client::new();
    let mut errors = Vec::new();

    for endpoint in endpoints {
        let response = match client.post(endpoint).body(query.clone()).send().await {
            Ok(response) => response,
            Err(err) => {
                errors.push(format!("{endpoint}: request failed: {err}"));
                continue;
            }
        };

        let response = match response.error_for_status() {
            Ok(ok) => ok,
            Err(err) => {
                let body = err
                    .status()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| "unknown status".to_owned());
                errors.push(format!("{endpoint}: HTTP error ({body})"));
                continue;
            }
        };

        let raw: serde_json::Value = response
            .json()
            .await
            .with_context(|| format!("{endpoint}: failed to decode JSON response"))?;

        if let Ok(geojson) = overpass_to_geojson(&raw) {
            return Ok(geojson);
        }

        errors.push(format!(
            "{endpoint}: response did not contain usable geometry"
        ));
    }

    bail!("all Overpass endpoints failed. {}", errors.join(" | "))
}

fn overpass_to_geojson(value: &serde_json::Value) -> Result<GeoJson> {
    let mut features = Vec::new();
    let elements = value
        .get("elements")
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow!("no elements in overpass response"))?;

    for element in elements {
        let tags = element.get("tags").cloned().unwrap_or_else(|| json!({}));
        let geometry = match element.get("geometry").and_then(|g| g.as_array()) {
            Some(geometry) => geometry,
            None => continue,
        };

        let coords: Vec<Vec<f64>> = geometry
            .iter()
            .filter_map(|point| {
                Some(vec![
                    point.get("lon")?.as_f64()?,
                    point.get("lat")?.as_f64()?,
                ])
            })
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

    if features.is_empty() {
        bail!("overpass response has no usable features");
    }

    Ok(GeoJson::FeatureCollection(geojson::FeatureCollection {
        bbox: None,
        features,
        foreign_members: None,
    }))
}

pub fn geojson_from_osm_xml(xml_bytes: &[u8]) -> Result<(GeoJson, OsmBounds)> {
    let mut reader = Reader::from_reader(xml_bytes);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut nodes: HashMap<i64, (f64, f64)> = HashMap::new();
    let mut ways: Vec<(Vec<i64>, JsonMap<String, JsonValue>)> = Vec::new();
    let mut in_way = false;
    let mut current_way_refs: Vec<i64> = Vec::new();
    let mut current_way_tags: JsonMap<String, JsonValue> = JsonMap::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) | Ok(Event::Empty(ref e)) if e.name().as_ref() == b"node" => {
                let mut id = None;
                let mut lat = None;
                let mut lon = None;
                for attr in e.attributes().flatten() {
                    match attr.key.as_ref() {
                        b"id" => {
                            if let Ok(text) = std::str::from_utf8(attr.value.as_ref()) {
                                id = text.parse::<i64>().ok();
                            }
                        }
                        b"lat" => {
                            if let Ok(text) = std::str::from_utf8(attr.value.as_ref()) {
                                lat = text.parse::<f64>().ok();
                            }
                        }
                        b"lon" => {
                            if let Ok(text) = std::str::from_utf8(attr.value.as_ref()) {
                                lon = text.parse::<f64>().ok();
                            }
                        }
                        _ => {}
                    }
                }
                if let (Some(id), Some(lat), Some(lon)) = (id, lat, lon) {
                    nodes.insert(id, (lat, lon));
                }
            }
            Ok(Event::Start(ref e)) if e.name().as_ref() == b"way" => {
                in_way = true;
                current_way_refs.clear();
                current_way_tags.clear();
            }
            Ok(Event::Empty(ref e)) if in_way && e.name().as_ref() == b"nd" => {
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"ref" {
                        let text = std::str::from_utf8(attr.value.as_ref()).unwrap_or_default();
                        if let Ok(parsed) = text.parse::<i64>() {
                            current_way_refs.push(parsed);
                        }
                    }
                }
            }
            Ok(Event::Empty(ref e)) if in_way && e.name().as_ref() == b"tag" => {
                let mut key = None;
                let mut value = None;
                for attr in e.attributes().flatten() {
                    if attr.key.as_ref() == b"k" {
                        key = std::str::from_utf8(attr.value.as_ref())
                            .ok()
                            .map(str::to_owned);
                    }
                    if attr.key.as_ref() == b"v" {
                        value = std::str::from_utf8(attr.value.as_ref())
                            .ok()
                            .map(str::to_owned);
                    }
                }
                if let (Some(key), Some(value)) = (key, value) {
                    current_way_tags.insert(key, JsonValue::String(value));
                }
            }
            Ok(Event::End(ref e)) if e.name().as_ref() == b"way" => {
                if !current_way_refs.is_empty() {
                    ways.push((current_way_refs.clone(), current_way_tags.clone()));
                }
                in_way = false;
            }
            Ok(Event::Eof) => break,
            Err(err) => return Err(anyhow!("invalid OSM XML: {err}")),
            _ => {}
        }

        buf.clear();
    }

    if ways.is_empty() {
        bail!("OSM XML does not contain usable <way> elements");
    }

    let mut features = Vec::new();
    let mut min_lat = f64::INFINITY;
    let mut max_lat = f64::NEG_INFINITY;
    let mut min_lon = f64::INFINITY;
    let mut max_lon = f64::NEG_INFINITY;

    for (refs, tags) in ways {
        let has_supported_tag = tags.contains_key("highway")
            || tags.contains_key("building")
            || tags.contains_key("landuse");
        if !has_supported_tag {
            continue;
        }

        let coords: Vec<Vec<f64>> = refs
            .into_iter()
            .filter_map(|id| nodes.get(&id).copied())
            .map(|(lat, lon)| {
                min_lat = min_lat.min(lat);
                max_lat = max_lat.max(lat);
                min_lon = min_lon.min(lon);
                max_lon = max_lon.max(lon);
                vec![lon, lat]
            })
            .collect();

        if coords.len() < 2 {
            continue;
        }

        let closed = coords.first() == coords.last();
        let is_area = closed && (tags.contains_key("building") || tags.contains_key("landuse"));
        let geom = if is_area && coords.len() > 3 {
            geojson::Geometry::new(Value::Polygon(vec![coords]))
        } else {
            geojson::Geometry::new(Value::LineString(coords))
        };

        features.push(Feature {
            geometry: Some(geom),
            properties: Some(tags),
            id: None,
            bbox: None,
            foreign_members: None,
        });
    }

    if features.is_empty() {
        bail!("OSM XML parsed, but no supported roads/buildings/landuse found");
    }

    let bounds = OsmBounds {
        north: max_lat,
        south: min_lat,
        east: max_lon,
        west: min_lon,
    };

    Ok((
        GeoJson::FeatureCollection(geojson::FeatureCollection {
            bbox: None,
            features,
            foreign_members: None,
        }),
        bounds,
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_osm_xml() {
        let xml = br#"<?xml version='1.0' encoding='UTF-8'?>
<osm version='0.6'>
  <node id='1' lat='55.0' lon='37.0'/>
  <node id='2' lat='55.1' lon='37.1'/>
  <node id='3' lat='55.2' lon='37.2'/>
  <way id='11'>
    <nd ref='1'/><nd ref='2'/><nd ref='3'/>
    <tag k='highway' v='residential'/>
  </way>
</osm>"#;

        let (geojson, bounds) = geojson_from_osm_xml(xml).unwrap();
        assert!(matches!(geojson, GeoJson::FeatureCollection(_)));
        assert!(bounds.north > bounds.south);
        assert!(bounds.east > bounds.west);
    }
}
