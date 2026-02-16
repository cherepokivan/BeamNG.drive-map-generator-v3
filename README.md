# BeamNG Real-Life Map Generator

Генератор карт для **BeamNG.drive** из данных реального мира:
- OSM (формы, дороги, объекты);
- AWS Terrain workflow (загрузка/подготовка heightmap-пайплайна);
- автоматическая генерация текстур;
- добавление road nodes;
- экспорт готового мода (`.zip`) в папку `Downloads`.

## Стек
- Backend: Rust (`axum` + `tokio`)
- Интерфейс: TypeScript + React (Vite)
- CI: GitHub Actions (автосборка portable `.exe`)

## Запуск backend
```bash
cargo run
```
API: `POST http://127.0.0.1:8080/api/generate`

Пример payload:
```json
{
  "map_name": "moscow_test",
  "north": 55.76,
  "south": 55.72,
  "east": 37.68,
  "west": 37.55,
  "texture_resolution": 1024
}
```

## Запуск интерфейса
```bash
cd ui
npm install
npm run dev
```

## Примечания
- Heightmap модуль в `src/aws_terrain.rs` сделан как интеграционный слой под AWS Terrain/S3 DEM пайплайн и сейчас содержит детерминированный генератор-заглушку для локальной разработки.
- Готовый архив мода сохраняется в системную папку `Downloads` (или в рабочую директорию, если путь недоступен).
