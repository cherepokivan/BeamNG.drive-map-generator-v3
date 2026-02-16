import React, { useMemo, useState } from "react";
import { createRoot } from "react-dom/client";
import { MapContainer, CircleMarker, Rectangle, TileLayer, useMapEvents } from "react-leaflet";
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

type MarkerSelectorProps = {
  onPointPick: (lat: number, lng: number) => void;
};

function MarkerSelector({ onPointPick }: MarkerSelectorProps) {
  useMapEvents({
    click(event) {
      onPointPick(event.latlng.lat, event.latlng.lng);
    },
  });
  return null;
}

function App() {
  const [form, setForm] = useState<GenerateRequest>({
    map_name: "arnis_style_city",
    north: 55.764,
    south: 55.736,
    east: 37.668,
    west: 37.548,
    texture_resolution: 1024,
  });
  const [marker, setMarker] = useState<[number, number]>([55.75, 37.61]);
  const [result, setResult] = useState<GenerateResponse | null>(null);
  const [status, setStatus] = useState("Готово");

  const bounds = useMemo<LatLngBoundsExpression>(
    () => [
      [form.south, form.west],
      [form.north, form.east],
    ],
    [form.south, form.west, form.north, form.east],
  );

  const updateBBoxFromCenter = (lat: number, lon: number) => {
    const latSpan = 0.02;
    const lonSpan = 0.03;

    setMarker([lat, lon]);
    setForm((prev) => ({
      ...prev,
      north: Number((lat + latSpan).toFixed(6)),
      south: Number((lat - latSpan).toFixed(6)),
      east: Number((lon + lonSpan).toFixed(6)),
      west: Number((lon - lonSpan).toFixed(6)),
    }));
  };

  const submit = async () => {
    setStatus("Генерация...");
    setResult(null);

    try {
      const res = await fetch("http://127.0.0.1:8080/api/generate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(form),
      });

      if (!res.ok) {
        setStatus(`Ошибка: ${await res.text()}`);
        return;
      }

      const payload = (await res.json()) as GenerateResponse;
      setResult(payload);
      setStatus("Готово");
    } catch (error) {
      setStatus(`Ошибка сети: ${(error as Error).message}`);
    }
  };

  return (
    <main
      style={{
        minHeight: "100vh",
        background: "radial-gradient(circle at 20% 20%, #1f2937 0%, #0b0f1a 45%, #05070e 100%)",
        color: "#dbeafe",
        fontFamily: "Inter, Segoe UI, sans-serif",
        padding: "24px",
      }}
    >
      <section
        style={{
          maxWidth: 1280,
          margin: "0 auto",
          display: "grid",
          gridTemplateColumns: "1fr 380px",
          gap: 20,
        }}
      >
        <article
          style={{
            border: "1px solid #233047",
            borderRadius: 16,
            overflow: "hidden",
            background: "#0b1220",
            boxShadow: "0 20px 45px rgba(0,0,0,0.35)",
          }}
        >
          <header style={{ padding: "14px 18px", borderBottom: "1px solid #233047" }}>
            <h1 style={{ margin: 0, fontSize: 22 }}>BeamNG Terrain Studio</h1>
            <p style={{ margin: "6px 0 0", color: "#93c5fd", fontSize: 14 }}>
              Стиль интерфейса вдохновлён Arnis: выбери точку на карте и сгенерируй мир.
            </p>
          </header>

          <MapContainer
            center={marker}
            zoom={12}
            style={{ height: 680, width: "100%" }}
            scrollWheelZoom
          >
            <TileLayer
              attribution='&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a>'
              url="https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png"
            />
            <CircleMarker center={marker} radius={8} pathOptions={{ color: "#f97316", fillColor: "#fb923c", fillOpacity: 0.9 }} />
            <Rectangle bounds={bounds} pathOptions={{ color: "#60a5fa", weight: 2 }} />
            <MarkerSelector onPointPick={updateBBoxFromCenter} />
          </MapContainer>
        </article>

        <aside
          style={{
            border: "1px solid #233047",
            borderRadius: 16,
            background: "linear-gradient(180deg, #0f172a 0%, #091222 100%)",
            padding: 18,
            boxShadow: "0 20px 45px rgba(0,0,0,0.35)",
          }}
        >
          <h2 style={{ marginTop: 0 }}>Параметры генерации</h2>

          {Object.entries(form).map(([key, value]) => (
            <label key={key} style={{ display: "block", marginBottom: 12, fontSize: 13, color: "#bfdbfe" }}>
              {key}
              <input
                style={{
                  display: "block",
                  width: "100%",
                  marginTop: 6,
                  padding: "10px 11px",
                  border: "1px solid #334155",
                  borderRadius: 10,
                  background: "#0b1220",
                  color: "#e2e8f0",
                  outline: "none",
                }}
                value={value}
                onChange={(event) =>
                  setForm((prev) => ({
                    ...prev,
                    [key]: key === "map_name" ? event.target.value : Number(event.target.value),
                  }))
                }
              />
            </label>
          ))}

          <button
            onClick={submit}
            style={{
              width: "100%",
              marginTop: 4,
              padding: "12px 14px",
              borderRadius: 12,
              border: "none",
              background: "linear-gradient(90deg, #2563eb 0%, #7c3aed 100%)",
              color: "#eff6ff",
              fontWeight: 700,
              cursor: "pointer",
            }}
          >
            Сгенерировать карту
          </button>

          <p style={{ marginTop: 12, color: "#93c5fd" }}>{status}</p>
          {result && (
            <pre
              style={{
                marginTop: 12,
                background: "#020617",
                borderRadius: 10,
                padding: 12,
                overflowX: "auto",
                border: "1px solid #1e293b",
              }}
            >
              {JSON.stringify(result, null, 2)}
            </pre>
          )}
        </aside>
      </section>
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
