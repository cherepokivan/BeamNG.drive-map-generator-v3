use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Result;
use tokio::fs;
use zip::write::SimpleFileOptions;

use crate::{
    model::GenerateMapRequest,
    model::RoadNode,
    osm::{OsmBuilding, OsmForestArea, OsmRoad},
    texture::TextureAssets,
};

pub async fn write_mod_archive(
    request: &GenerateMapRequest,
    build_root: &Path,
    heightmap_path: &Path,
    textures: &TextureAssets,
    roads: &[OsmRoad],
    buildings: &[OsmBuilding],
    forests: &[OsmForestArea],
) -> Result<PathBuf> {
    let package_root = build_root.join("package_root");
    let mod_root = package_root.join("levels").join(&request.map_name);
    let art_root = mod_root.join("art").join("shapes").join("generated");
    fs::create_dir_all(&mod_root).await?;
    fs::create_dir_all(&art_root).await?;

    let road_nodes: Vec<RoadNode> = roads.iter().flat_map(|r| r.points.clone()).collect();
    fs::write(
        mod_root.join("road_nodes.json"),
        serde_json::to_vec_pretty(&road_nodes)?,
    )
    .await?;

    fs::copy(heightmap_path, mod_root.join("heightmap.raw")).await?;
    fs::copy(
        &textures.terrain_albedo,
        mod_root.join("terrain_albedo.png"),
    )
    .await?;
    fs::copy(
        &textures.terrain_normal,
        mod_root.join("terrain_normal.png"),
    )
    .await?;
    fs::copy(
        &textures.terrain_roughness,
        mod_root.join("terrain_roughness.png"),
    )
    .await?;

    fs::copy(
        &textures.building_wall_albedo,
        art_root.join("building_wall_albedo.png"),
    )
    .await?;
    fs::copy(
        &textures.building_wall_normal,
        art_root.join("building_wall_normal.png"),
    )
    .await?;
    fs::copy(
        &textures.building_roof_albedo,
        art_root.join("building_roof_albedo.png"),
    )
    .await?;
    fs::copy(
        &textures.building_roof_normal,
        art_root.join("building_roof_normal.png"),
    )
    .await?;

    fs::write(
        mod_root.join("main.materials.json"),
        serde_json::to_vec_pretty(&main_materials_json(request))?,
    )
    .await?;

    fs::write(
        mod_root.join("generated_roads.json"),
        serde_json::to_vec_pretty(&build_roads_json(request, roads))?,
    )
    .await?;

    fs::write(
        mod_root.join("generated_ai_paths.json"),
        serde_json::to_vec_pretty(&build_ai_paths_json(request, roads))?,
    )
    .await?;

    fs::write(
        art_root.join("buildings.dae"),
        build_buildings_dae(request, buildings),
    )
    .await?;

    fs::write(
        mod_root.join("generated_buildings.prefab.json"),
        serde_json::to_vec_pretty(&build_building_prefab_json(request, buildings.len()))?,
    )
    .await?;

    fs::write(
        mod_root.join("generated_trees.prefab.json"),
        serde_json::to_vec_pretty(&build_trees_prefab_json(request, forests))?,
    )
    .await?;

    let level_info = serde_json::json!({
      "name": request.map_name,
      "generatedBy": "beamng-map-generator",
      "description": "Generated from OSM + AWS terrain workflow",
      "roads": roads.len(),
      "buildings": buildings.len(),
      "forestAreas": forests.len()
    });
    fs::write(
        mod_root.join("info.json"),
        serde_json::to_vec_pretty(&level_info)?,
    )
    .await?;

    let mod_id = build_mod_id(&request.map_name);
    let mod_info_root = package_root.join("mod_info").join(&mod_id);
    fs::create_dir_all(mod_info_root.join("images")).await?;
    fs::create_dir_all(mod_info_root.join("thumbs")).await?;
    let mod_info = serde_json::json!({
      "name": format!("{} (Generated)", request.map_name),
      "author": "beamng-map-generator",
      "version": "1.0",
      "description": "Auto-generated map mod",
      "tags": ["Map", "Generated"],
      "type": "mod"
    });
    fs::write(
        mod_info_root.join("info.json"),
        serde_json::to_vec_pretty(&mod_info)?,
    )
    .await?;

    let downloads = dirs::download_dir().unwrap_or_else(|| build_root.to_path_buf());
    fs::create_dir_all(&downloads).await?;
    let zip_path = downloads.join(format!("{}_beamng_mod.zip", request.map_name));

    let file = std::fs::File::create(&zip_path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    add_dir_to_zip(&mut zip, &package_root, "", options)?;

    zip.finish()?;

    Ok(zip_path)
}

fn main_materials_json(request: &GenerateMapRequest) -> serde_json::Value {
    serde_json::json!({
      "materials": [
        {
          "name": format!("{}_terrain", request.map_name),
          "mapTo": format!("{}_terrain", request.map_name),
          "class": "TerrainMaterial",
          "diffuseMap": "terrain_albedo.png",
          "normalMap": "terrain_normal.png",
          "roughnessMap": "terrain_roughness.png",
          "detailScale": 8
        },
        {
          "name": format!("{}_building_wall", request.map_name),
          "mapTo": format!("{}_building_wall", request.map_name),
          "class": "Material",
          "diffuseMap": "art/shapes/generated/building_wall_albedo.png",
          "normalMap": "art/shapes/generated/building_wall_normal.png"
        },
        {
          "name": format!("{}_building_roof", request.map_name),
          "mapTo": format!("{}_building_roof", request.map_name),
          "class": "Material",
          "diffuseMap": "art/shapes/generated/building_roof_albedo.png",
          "normalMap": "art/shapes/generated/building_roof_normal.png"
        }
      ]
    })
}

fn build_roads_json(request: &GenerateMapRequest, roads: &[OsmRoad]) -> serde_json::Value {
    let map_width_m = 2048.0;
    let map_height_m = 2048.0;

    let road_entries: Vec<serde_json::Value> = roads
        .iter()
        .enumerate()
        .filter_map(|(index, road)| {
            if road.points.len() < 2 {
                return None;
            }

            let width = match road.highway_type.as_str() {
                "motorway" => 12.0,
                "primary" => 10.0,
                "secondary" => 8.0,
                _ => 6.0,
            };

            let nodes: Vec<serde_json::Value> = road
                .points
                .iter()
                .map(|p| {
                    let (x, y) = geo_to_local(request, p.lat, p.lon, map_width_m, map_height_m);
                    serde_json::json!([x, y, 0.4])
                })
                .collect();

            Some(serde_json::json!({
              "class": "DecalRoad",
              "name": format!("road_{index}"),
              "material": "road_asphalt_2lane",
              "drivability": 1,
              "improvedSpline": true,
              "nodes": nodes,
              "width": width
            }))
        })
        .collect();

    serde_json::json!({ "roads": road_entries })
}

fn build_ai_paths_json(request: &GenerateMapRequest, roads: &[OsmRoad]) -> serde_json::Value {
    let map_width_m = 2048.0;
    let map_height_m = 2048.0;
    let paths: Vec<serde_json::Value> = roads
        .iter()
        .enumerate()
        .filter(|(_, r)| r.points.len() >= 2)
        .map(|(i, r)| {
            let nodes: Vec<serde_json::Value> = r
                .points
                .iter()
                .map(|p| {
                    let (x, y) = geo_to_local(request, p.lat, p.lon, map_width_m, map_height_m);
                    serde_json::json!({"pos": [x, y, 0.3], "radius": 4.0})
                })
                .collect();
            serde_json::json!({"name": format!("ai_path_{i}"), "nodes": nodes})
        })
        .collect();

    serde_json::json!({"aiPaths": paths})
}

fn build_buildings_dae(request: &GenerateMapRequest, buildings: &[OsmBuilding]) -> String {
    let map_width_m = 2048.0;
    let map_height_m = 2048.0;

    let mut positions = Vec::<[f32; 3]>::new();
    let mut tris = Vec::<[usize; 3]>::new();

    for building in buildings {
        if building.points.len() < 4 {
            continue;
        }

        let base_height = 0.5_f32;
        let top_height = base_height + building.levels * 3.2;

        let mut base = Vec::<[f32; 3]>::new();
        for p in &building.points {
            let (x, y) = geo_to_local(request, p.lat, p.lon, map_width_m, map_height_m);
            base.push([x as f32, y as f32, base_height]);
        }

        if base.first() == base.last() {
            base.pop();
        }
        if base.len() < 3 {
            continue;
        }

        let offset = positions.len();
        for b in &base {
            positions.push(*b);
        }
        for b in &base {
            positions.push([b[0], b[1], top_height]);
        }

        for i in 0..base.len() {
            let n = (i + 1) % base.len();
            let b0 = offset + i;
            let b1 = offset + n;
            let t0 = offset + base.len() + i;
            let t1 = offset + base.len() + n;

            tris.push([b0, b1, t1]);
            tris.push([b0, t1, t0]);
        }

        for i in 1..(base.len() - 1) {
            tris.push([
                offset + base.len(),
                offset + base.len() + i,
                offset + base.len() + i + 1,
            ]);
        }
    }

    let mut pos_text = String::new();
    for p in &positions {
        pos_text.push_str(&format!("{} {} {} ", p[0], p[1], p[2]));
    }

    let mut tri_text = String::new();
    for t in &tris {
        tri_text.push_str(&format!("{} {} {} ", t[0], t[1], t[2]));
    }

    format!(
        r##"<?xml version="1.0" encoding="utf-8"?>
<COLLADA xmlns="http://www.collada.org/2005/11/COLLADASchema" version="1.4.1">
  <asset><up_axis>Z_UP</up_axis></asset>
  <library_geometries>
    <geometry id="buildingsGeo" name="buildingsGeo">
      <mesh>
        <source id="buildingsGeo-positions">
          <float_array id="buildingsGeo-positions-array" count="{pos_count}">{positions}</float_array>
          <technique_common>
            <accessor source="#buildingsGeo-positions-array" count="{vert_count}" stride="3">
              <param name="X" type="float"/>
              <param name="Y" type="float"/>
              <param name="Z" type="float"/>
            </accessor>
          </technique_common>
        </source>
        <vertices id="buildingsGeo-vertices">
          <input semantic="POSITION" source="#buildingsGeo-positions"/>
        </vertices>
        <triangles count="{tri_count}" material="buildingWallMat">
          <input semantic="VERTEX" source="#buildingsGeo-vertices" offset="0"/>
          <p>{triangles}</p>
        </triangles>
      </mesh>
    </geometry>
  </library_geometries>
  <library_visual_scenes>
    <visual_scene id="Scene" name="Scene">
      <node id="buildingsNode" name="buildingsNode">
        <instance_geometry url="#buildingsGeo"/>
      </node>
    </visual_scene>
  </library_visual_scenes>
  <scene><instance_visual_scene url="#Scene"/></scene>
</COLLADA>"##,
        pos_count = positions.len() * 3,
        vert_count = positions.len(),
        tri_count = tris.len(),
        positions = pos_text,
        triangles = tri_text
    )
}

fn build_building_prefab_json(
    request: &GenerateMapRequest,
    building_count: usize,
) -> serde_json::Value {
    serde_json::json!({
      "class": "Prefab",
      "name": format!("{}_generated_buildings", request.map_name),
      "shapeName": "art/shapes/generated/buildings.dae",
      "instanceCount": building_count,
      "position": [0.0, 0.0, 0.0],
      "scale": [1.0, 1.0, 1.0]
    })
}

fn build_trees_prefab_json(
    request: &GenerateMapRequest,
    forests: &[OsmForestArea],
) -> serde_json::Value {
    let map_width_m = 2048.0;
    let map_height_m = 2048.0;
    let mut instances = Vec::new();

    for (area_idx, area) in forests.iter().enumerate() {
        for (i, p) in area.points.iter().enumerate().step_by(2) {
            let (x, y) = geo_to_local(request, p.lat, p.lon, map_width_m, map_height_m);
            instances.push(serde_json::json!({
              "name": format!("tree_{}_{}", area_idx, i),
              "class": "TSStatic",
              "shapeName": "art/shapes/trees/pine/pine_01.dae",
              "position": [x, y, 0.5],
              "scale": [1.0, 1.0, 1.0]
            }));
        }
    }

    serde_json::json!({"instances": instances})
}

fn build_mod_id(map_name: &str) -> String {
    let clean: String = map_name
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    let head = if clean.is_empty() {
        "GENERATED".to_owned()
    } else {
        clean.chars().take(8).collect::<String>()
    };
    format!("{}{}", head.to_uppercase(), "AUTO")
}

fn geo_to_local(
    request: &GenerateMapRequest,
    lat: f64,
    lon: f64,
    width_m: f64,
    height_m: f64,
) -> (f64, f64) {
    let x = ((lon - request.west) / (request.east - request.west)).clamp(0.0, 1.0) * width_m
        - width_m / 2.0;
    let y = ((lat - request.south) / (request.north - request.south)).clamp(0.0, 1.0) * height_m
        - height_m / 2.0;
    (x, y)
}

fn add_dir_to_zip(
    zip: &mut zip::ZipWriter<std::fs::File>,
    src_root: &Path,
    zip_root: &str,
    options: SimpleFileOptions,
) -> Result<()> {
    for entry in walk_dir(src_root)? {
        let rel = entry.strip_prefix(src_root)?;
        let rel_path = rel.to_string_lossy().replace('\\', "/");
        let zip_path = if zip_root.is_empty() {
            rel_path
        } else {
            format!("{zip_root}/{rel_path}")
        };

        if entry.is_file() {
            zip.start_file(zip_path, options)?;
            let bytes = std::fs::read(&entry)?;
            zip.write_all(&bytes)?;
        }
    }
    Ok(())
}

fn walk_dir(root: &Path) -> Result<Vec<PathBuf>> {
    let mut all = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            all.extend(walk_dir(&path)?);
        } else {
            all.push(path);
        }
    }
    Ok(all)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::texture::TextureAssets;

    #[tokio::test]
    async fn creates_zip() {
        let temp = tempfile::tempdir().unwrap();
        let h = temp.path().join("h.raw");
        tokio::fs::write(&h, [0_u8; 8]).await.unwrap();

        let tex = TextureAssets {
            terrain_albedo: temp.path().join("terrain_albedo.png"),
            terrain_normal: temp.path().join("terrain_normal.png"),
            terrain_roughness: temp.path().join("terrain_roughness.png"),
            building_wall_albedo: temp.path().join("building_wall_albedo.png"),
            building_wall_normal: temp.path().join("building_wall_normal.png"),
            building_roof_albedo: temp.path().join("building_roof_albedo.png"),
            building_roof_normal: temp.path().join("building_roof_normal.png"),
        };
        for p in [
            &tex.terrain_albedo,
            &tex.terrain_normal,
            &tex.terrain_roughness,
            &tex.building_wall_albedo,
            &tex.building_wall_normal,
            &tex.building_roof_albedo,
            &tex.building_roof_normal,
        ] {
            tokio::fs::write(p, [1_u8; 8]).await.unwrap();
        }

        let req = GenerateMapRequest {
            map_name: "test".into(),
            north: 1.0,
            south: 0.0,
            east: 1.0,
            west: 0.0,
            texture_resolution: 256,
        };

        let out = write_mod_archive(&req, temp.path(), &h, &tex, &[], &[], &[])
            .await
            .unwrap();
        assert!(out.exists());

        let file = std::fs::File::open(&out).unwrap();
        let mut zip = zip::ZipArchive::new(file).unwrap();
        let mut found_levels = false;
        let mut found_mod_info = false;
        let mut has_leading_slash = false;
        for i in 0..zip.len() {
            let name = zip.by_index(i).unwrap().name().to_string();
            if name.starts_with('/') {
                has_leading_slash = true;
            }
            if name.starts_with("levels/test/") {
                found_levels = true;
            }
            if name.starts_with("mod_info/") && name.ends_with("/info.json") {
                found_mod_info = true;
            }
        }
        assert!(found_levels);
        assert!(found_mod_info);
        assert!(!has_leading_slash);
    }
}
