import React, { useState } from "react";
import { createRoot } from "react-dom/client";

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

function App() {
  const [form, setForm] = useState<GenerateRequest>({
    map_name: "moscow_test",
    north: 55.76,
    south: 55.72,
    east: 37.68,
    west: 37.55,
    texture_resolution: 1024,
  });
  const [result, setResult] = useState<GenerateResponse | null>(null);
  const [status, setStatus] = useState("Готово");

  const submit = async () => {
    setStatus("Генерация...");
    setResult(null);
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
  };

  return (
    <main style={{ maxWidth: 640, margin: "32px auto", fontFamily: "sans-serif" }}>
      <h1>BeamNG Real-Life Map Generator</h1>
      <p>OSM объекты + AWS terrain + текстуры + road nodes + экспорт .zip в Downloads</p>

      {Object.entries(form).map(([key, value]) => (
        <label key={key} style={{ display: "block", marginBottom: 12 }}>
          {key}
          <input
            style={{ display: "block", width: "100%", padding: 8 }}
            value={value}
            onChange={(e) =>
              setForm((prev) => ({
                ...prev,
                [key]: key === "map_name" ? e.target.value : Number(e.target.value),
              }))
            }
          />
        </label>
      ))}

      <button onClick={submit}>Сгенерировать карту</button>
      <p>{status}</p>
      {result && (
        <pre>{JSON.stringify(result, null, 2)}</pre>
      )}
    </main>
  );
}

createRoot(document.getElementById("root")!).render(<App />);
