import React, { useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { CircleMarker, MapContainer, Rectangle, TileLayer, useMapEvents } from "react-leaflet";
import type { LatLngBoundsExpression } from "leaflet";
import "leaflet/dist/leaflet.css";

type GenerateRequest = {
  map_name: string;
  north: number;
  south: number;
  east: number;
  west: number;
  texture_resolution: number;
};

type GenerateResponse = {
  map_name: string;
  mod_archive: string;
  road_nodes_count: number;
};

type BBox = {
  north: number;
  south: number;
  east: number;
  west: number;
};

const TEXTURE_QUALITY_LEVELS = [
  { value: 512, label: "Очень низко — неиграбельно" },
  { value: 1024, label: "Низко — на грани играбельности" },
  { value: 2048, label: "Средне — играбельно" },
  { value: 4096, label: "Чётко — комфортно играбельно" },
  { value: 8192, label: "Очень чётко — максимально играбельно" },
] as const;

function buildBox(aLat: number, aLng: number, bLat: number, bLng: number): BBox {
  return {
    north: Math.max(aLat, bLat),
    south: Math.min(aLat, bLat),
    east: Math.max(aLng, bLng),
    west: Math.min(aLng, bLng),
  };
}

function isBoxValid(box: BBox): boolean {
  return box.north - box.south > 0.0001 && box.east - box.west > 0.0001;
}

function AreaSelector({
  drawMode,
  onCenterPick,
  onAreaPreview,
  onAreaFinish,
}: {
  drawMode: boolean;
  onCenterPick: (lat: number, lng: number) => void;
  onAreaPreview: (box: BBox | null) => void;
  onAreaFinish: (box: BBox | null) => void;
}) {
  const [dragStart, setDragStart] = useState<{ lat: number; lng: number } | null>(null);

  useMapEvents({
    click(event) {
      if (!drawMode) {
        onCenterPick(event.latlng.lat, event.latlng.lng);
      }
    },
    mousedown(event) {
      if (!drawMode) return;
      setDragStart({ lat: event.latlng.lat, lng: event.latlng.lng });
      onAreaPreview(null);
    },
    mousemove(event) {
      if (!drawMode || !dragStart) return;
      onAreaPreview(buildBox(dragStart.lat, dragStart.lng, event.latlng.lat, event.latlng.lng));
    },
    mouseup(event) {
      if (!drawMode || !dragStart) return;
      const completed = buildBox(dragStart.lat, dragStart.lng, event.latlng.lat, event.latlng.lng);
      setDragStart(null);
      onAreaPreview(null);
      onAreaFinish(isBoxValid(completed) ? completed : null);
    },
  });

  return null;
}

function App() {
  const [mapName, setMapName] = useState("arnis_style_city");
  const [textureResolution, setTextureResolution] = useState(1024);
  const [sourceMode, setSourceMode] = useState<"online" | "local">("online");
  const [osmFile, setOsmFile] = useState<File | null>(null);

  const [marker, setMarker] = useState<[number, number]>([55.75, 37.61]);
  const [bbox, setBbox] = useState<BBox | null>(null);
  const [previewBox, setPreviewBox] = useState<BBox | null>(null);
  const [drawMode, setDrawMode] = useState(false);

  const [result, setResult] = useState<GenerateResponse | null>(null);
  const [status, setStatus] = useState("Готово");

  const bounds = useMemo<LatLngBoundsExpression | null>(
    () => (bbox ? [[bbox.south, bbox.west], [bbox.north, bbox.east]] : null),
    [bbox],
  );
  const previewBounds = useMemo<LatLngBoundsExpression | null>(
    () => (previewBox ? [[previewBox.south, previewBox.west], [previewBox.north, previewBox.east]] : null),
    [previewBox],
  );

  const submit = async () => {
    setStatus("Генерация...");
    setResult(null);

    try {
      let response: Response;

      if (sourceMode === "local") {
        if (!osmFile) {
          setStatus("Выберите локальный .osm файл (Экспорт из OpenStreetMap).");
          return;
        }

        const form = new FormData();
        form.append("map_name", mapName);
        form.append("texture_resolution", String(textureResolution));
        form.append("osm_file", osmFile);

        response = await fetch("/api/generate-from-osm", {
          method: "POST",
          body: form,
        });
      } else {
        if (!bbox) {
          setStatus("Сначала выделите область на карте (режим Выбор области).");
          return;
        }

        const payload: GenerateRequest = {
          map_name: mapName,
          texture_resolution: textureResolution,
          ...bbox,
        };

        response = await fetch("/api/generate", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(payload),
        });
      }

      if (!response.ok) {
        setStatus(`Ошибка: ${await response.text()}`);
        return;
      }

      const body = (await response.json()) as GenerateResponse;
      setResult(body);
      setStatus("Готово");
    } catch (error) {
      setStatus(`Ошибка сети: ${(error as Error).message}`);
    }
  };

  return (
    <main style={{ minHeight: "100vh", background: "radial-gradient(circle at 20% 20%, #1f2937 0%, #0b0f1a 45%, #05070e 100%)", color: "#dbeafe", fontFamily: "Inter, Segoe UI, sans-serif", padding: 24 }}>
      <section style={{ maxWidth: 1280, margin: "0 auto", display: "grid", gridTemplateColumns: "1fr 380px", gap: 20 }}>
        <article style={{ border: "1px solid #233047", borderRadius: 16, overflow: "hidden", background: "#0b1220", boxShadow: "0 20px 45px rgba(0,0,0,0.35)" }}>
          <header style={{ padding: "14px 18px", borderBottom: "1px solid #233047" }}>
            <h1 style={{ margin: 0, fontSize: 22 }}>BeamNG Terrain Studio</h1>
            <p style={{ margin: "6px 0 0", color: "#93c5fd", fontSize: 14 }}>
              Онлайн: выделите область на карте. Локально: загрузите экспортированный .osm файл. Максимум качества текстур: 8192.
            </p>
          </header>

          <MapContainer center={marker} zoom={12} style={{ height: 680, width: "100%" }} scrollWheelZoom>
            <TileLayer attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>' url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png" />
            <CircleMarker center={marker} radius={8} pathOptions={{ color: "#f97316", fillColor: "#fb923c", fillOpacity: 0.9 }} />
            {bounds && <Rectangle bounds={bounds} pathOptions={{ color: "#60a5fa", weight: 2 }} />}
            {previewBounds && <Rectangle bounds={previewBounds} pathOptions={{ color: "#22d3ee", weight: 2, dashArray: "6 6" }} />}

            <AreaSelector
              drawMode={sourceMode === "online" && drawMode}
              onCenterPick={(lat, lon) => setMarker([lat, lon])}
              onAreaPreview={setPreviewBox}
              onAreaFinish={(nextBox) => {
                if (!nextBox) {
                  setStatus("Слишком маленькая область. Выделите прямоугольник побольше.");
                  return;
                }
                setBbox(nextBox);
                setMarker([(nextBox.north + nextBox.south) / 2, (nextBox.east + nextBox.west) / 2]);
                setStatus("Область выбрана.");
              }}
            />
          </MapContainer>
        </article>

        <aside style={{ border: "1px solid #233047", borderRadius: 16, background: "linear-gradient(180deg, #0f172a 0%, #091222 100%)", padding: 18, boxShadow: "0 20px 45px rgba(0,0,0,0.35)" }}>
          <h2 style={{ marginTop: 0 }}>Параметры генерации</h2>

          <label style={{ display: "block", marginBottom: 12, fontSize: 13, color: "#bfdbfe" }}>
            map_name
            <input style={inputStyle} value={mapName} onChange={(event) => setMapName(event.target.value)} />
          </label>

          <label style={{ display: "block", marginBottom: 12, fontSize: 13, color: "#bfdbfe" }}>
            texture_resolution
            <select
              style={inputStyle}
              value={textureResolution}
              onChange={(event) => setTextureResolution(Number(event.target.value))}
            >
              {TEXTURE_QUALITY_LEVELS.map((level) => (
                <option key={level.value} value={level.value}>
                  {level.value} — {level.label}
                </option>
              ))}
            </select>
            <div style={{ marginTop: 6, fontSize: 12, color: "#93c5fd" }}>
              Текущий уровень: {textureResolution} — {TEXTURE_QUALITY_LEVELS.find((l) => l.value === textureResolution)?.label}
            </div>
          </label>

          <div style={{ marginBottom: 12 }}>
            <div style={{ marginBottom: 8, fontSize: 13, color: "#bfdbfe" }}>Источник OSM данных</div>
            <label style={{ display: "block", fontSize: 13, marginBottom: 6 }}>
              <input type="radio" checked={sourceMode === "online"} onChange={() => setSourceMode("online")} /> Онлайн (Overpass)
            </label>
            <label style={{ display: "block", fontSize: 13 }}>
              <input type="radio" checked={sourceMode === "local"} onChange={() => setSourceMode("local")} /> Локальный .osm файл
            </label>
          </div>

          {sourceMode === "online" ? (
            <button
              onClick={() => {
                setDrawMode((prev) => !prev);
                setPreviewBox(null);
                setStatus(drawMode ? "Режим выбора отключён." : "Режим выбора включён: выделите область на карте.");
              }}
              style={{ ...buttonStyle, marginBottom: 10, background: drawMode ? "linear-gradient(90deg, #ef4444 0%, #f97316 100%)" : "linear-gradient(90deg, #0891b2 0%, #2563eb 100%)" }}
            >
              {drawMode ? "Выключить выбор области" : "Включить выбор области"}
            </button>
          ) : (
            <label style={{ display: "block", marginBottom: 10, fontSize: 13, color: "#bfdbfe" }}>
              Выберите .osm файл
              <input
                style={inputStyle}
                type="file"
                accept=".osm,text/xml,application/xml"
                onChange={(event) => setOsmFile(event.target.files?.[0] ?? null)}
              />
            </label>
          )}

          <div style={{ marginBottom: 12, fontSize: 12, color: "#93c5fd", lineHeight: 1.5 }}>
            Центр: {marker[0].toFixed(5)}, {marker[1].toFixed(5)}
            <br />
            Область: {bbox ? "выбрана" : "не выбрана"}
            <br />
            Файл: {osmFile ? osmFile.name : "не выбран"}
          </div>

          <button onClick={submit} style={buttonStyle}>Сгенерировать карту</button>

          <p style={{ marginTop: 12, color: "#93c5fd" }}>{status}</p>
          {result && <pre style={resultStyle}>{JSON.stringify(result, null, 2)}</pre>}
        </aside>
      </section>
    </main>
  );
}

const inputStyle: React.CSSProperties = {
  display: "block",
  width: "100%",
  marginTop: 6,
  padding: "10px 11px",
  border: "1px solid #334155",
  borderRadius: 10,
  background: "#0b1220",
  color: "#e2e8f0",
  outline: "none",
};

const buttonStyle: React.CSSProperties = {
  width: "100%",
  marginTop: 4,
  padding: "12px 14px",
  borderRadius: 12,
  border: "none",
  background: "linear-gradient(90deg, #2563eb 0%, #7c3aed 100%)",
  color: "#eff6ff",
  fontWeight: 700,
  cursor: "pointer",
};

const resultStyle: React.CSSProperties = {
  marginTop: 12,
  background: "#020617",
  borderRadius: 10,
  padding: 12,
  overflowX: "auto",
  border: "1px solid #1e293b",
};

createRoot(document.getElementById("root")!).render(<App />);
