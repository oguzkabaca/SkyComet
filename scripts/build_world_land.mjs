// Regenerate frontend/src/viz/worldLand.ts from a Natural Earth land GeoJSON.
//
// Source (public domain):
//   https://raw.githubusercontent.com/nvkelso/natural-earth-vector/master/geojson/ne_110m_land.geojson
//
// Usage:
//   node scripts/build_world_land.mjs path/to/ne_110m_land.geojson
//
// Each land polygon is projected with the same equirectangular geometry as
// WorldMap.project() (canon docs/calculations.md §7.4) into viewBox units, so
// the vector shares the exact coordinate space of the ground-track overlays.
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, resolve } from 'node:path';

const WIDTH = 720;
const HEIGHT = 360;
const MARGIN = 12;
const PLOT_W = WIDTH - 2 * MARGIN;
const PLOT_H = HEIGHT - 2 * MARGIN;

const px = (lon, lat) =>
  (MARGIN + ((lon + 180) / 360) * PLOT_W).toFixed(1) +
  ' ' +
  (MARGIN + ((90 - lat) / 180) * PLOT_H).toFixed(1);

const src = process.argv[2];
if (!src) {
  console.error('usage: node scripts/build_world_land.mjs <land.geojson>');
  process.exit(1);
}

const gj = JSON.parse(readFileSync(src, 'utf8'));
const rings = [];

function addRings(coords) {
  for (const ring of coords) {
    let d = '';
    let prev = '';
    for (const [lon, lat] of ring) {
      const p = px(lon, lat);
      if (p === prev) continue; // drop points that collapse at this precision
      d += (d === '' ? 'M' : 'L') + p;
      prev = p;
    }
    if (d) rings.push(d + 'Z');
  }
}

for (const f of gj.features) {
  const g = f.geometry;
  if (g.type === 'Polygon') addRings(g.coordinates);
  else if (g.type === 'MultiPolygon') for (const poly of g.coordinates) addRings(poly);
}

const path = rings.join('');
const out = resolve(dirname(fileURLToPath(import.meta.url)), '../frontend/src/viz/worldLand.ts');
const header =
  '// Auto-generated — do NOT edit by hand.\n' +
  '// Source: Natural Earth 110m land (public domain, naturalearthdata.com).\n' +
  '// Equirectangular land polygons in WorldMap viewBox units (720x360), built\n' +
  '// with the same project() geometry as canon docs/calculations.md §7.4.\n' +
  '// Regenerate: node scripts/build_world_land.mjs <land.geojson>\n' +
  "export const WORLD_LAND_PATH =\n  '";
writeFileSync(out, header + path + "';\n");
console.log(`wrote ${out} (${rings.length} rings, ${path.length} chars)`);
