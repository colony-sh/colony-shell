#!/usr/bin/env python3
import argparse
import json
from pathlib import Path


def load_features(path: Path):
    data = json.loads(path.read_text())
    if not isinstance(data, dict) or "features" not in data:
        raise SystemExit("features.json missing 'features'")
    return data["features"]


def generate_html(features):
    features_json = json.dumps(features)
    return f"""<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1" />
<title>Colony Shell Roadmap</title>
<style>
:root {{
  --bg: #0f141b;
  --panel: #151b24;
  --text: #e6edf3;
  --muted: #9aa4b2;
  --accent: #68c0f7;
  --border: #2a3340;
  --chip: #223043;
}}
* {{ box-sizing: border-box; }}
body {{ margin: 0; font-family: ui-sans-serif, system-ui, -apple-system, Segoe UI, Roboto; background: var(--bg); color: var(--text); }}
header {{ padding: 16px 20px; border-bottom: 1px solid var(--border); background: var(--panel); position: sticky; top: 0; z-index: 2; }}
header h1 {{ margin: 0; font-size: 18px; }}
.controls {{ display: grid; grid-template-columns: 1fr 160px 160px 160px; gap: 12px; margin-top: 12px; }}
input, select {{ background: #0d1117; color: var(--text); border: 1px solid var(--border); padding: 8px 10px; border-radius: 8px; }}
main {{ padding: 16px 20px; }}
.board {{ display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 16px; }}
.column {{ background: var(--panel); border: 1px solid var(--border); border-radius: 12px; overflow: hidden; }}
.column h2 {{ margin: 0; padding: 12px 14px; font-size: 14px; text-transform: uppercase; letter-spacing: 0.08em; background: #0d1117; border-bottom: 1px solid var(--border); }}
.card {{ padding: 12px 14px; border-bottom: 1px solid var(--border); }}
.card:last-child {{ border-bottom: none; }}
.card h3 {{ margin: 0 0 6px 0; font-size: 14px; }}
.card p {{ margin: 0; color: var(--muted); font-size: 12px; }}
.meta {{ margin-top: 8px; display: flex; gap: 8px; flex-wrap: wrap; }}
.chip {{ background: var(--chip); border: 1px solid var(--border); color: var(--muted); padding: 2px 6px; border-radius: 999px; font-size: 11px; }}
.chip.accent {{ color: var(--accent); border-color: #1f3b56; }}
.notes {{ margin-top: 8px; font-size: 11px; color: var(--muted); }}
</style>
</head>
<body>
<header>
  <h1>Colony Shell Roadmap</h1>
  <div class="controls">
    <input id="search" placeholder="Search by name, id, or notes" />
    <select id="status">
      <option value="">All Status</option>
      <option value="new">new</option>
      <option value="in_progress">in_progress</option>
      <option value="complete">complete</option>
      <option value="blocked">blocked</option>
    </select>
    <select id="priority">
      <option value="">All Priority</option>
      <option value="1">1</option>
      <option value="2">2</option>
      <option value="3">3</option>
      <option value="4">4</option>
    </select>
    <select id="group">
      <option value="status">Group by status</option>
      <option value="priority">Group by priority</option>
    </select>
  </div>
</header>
<main>
  <div id="board" class="board"></div>
</main>
<script>
const FEATURES = {features_json};
const board = document.getElementById('board');
const search = document.getElementById('search');
const statusFilter = document.getElementById('status');
const priorityFilter = document.getElementById('priority');
const groupBy = document.getElementById('group');

function matches(feature) {{
  const q = search.value.trim().toLowerCase();
  if (q) {{
    const hay = [feature.id, feature.name, feature.description, ...(feature.notes || [])].join(' ').toLowerCase();
    if (!hay.includes(q)) return false;
  }}
  if (statusFilter.value && feature.status !== statusFilter.value) return false;
  if (priorityFilter.value && String(feature.priority) !== priorityFilter.value) return false;
  return true;
}}

function groupKey(feature) {{
  if (groupBy.value === 'priority') return `priority ${{feature.priority}}`;
  return feature.status || 'unknown';
}}

function render() {{
  const groups = new Map();
  FEATURES.filter(matches).forEach(f => {{
    const key = groupKey(f);
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key).push(f);
  }});

  const keys = Array.from(groups.keys()).sort((a, b) => a.localeCompare(b));
  board.innerHTML = '';
  keys.forEach(key => {{
    const column = document.createElement('div');
    column.className = 'column';
    const title = document.createElement('h2');
    title.textContent = key;
    column.appendChild(title);

    groups.get(key).sort((a, b) => a.priority - b.priority).forEach(f => {{
      const card = document.createElement('div');
      card.className = 'card';
      const h3 = document.createElement('h3');
      h3.textContent = `${{f.id}} · ${{f.name}}`;
      const p = document.createElement('p');
      p.textContent = f.description || '';
      const meta = document.createElement('div');
      meta.className = 'meta';
      meta.appendChild(chip(`priority ${{f.priority}}`, true));
      meta.appendChild(chip(f.status));
      if (f.dependencies && f.dependencies.length) {{
        meta.appendChild(chip(`deps: ${{f.dependencies.join(', ')}}`));
      }}
      const notes = document.createElement('div');
      notes.className = 'notes';
      notes.textContent = (f.notes || []).slice(-3).join(' • ');
      card.appendChild(h3);
      card.appendChild(p);
      card.appendChild(meta);
      if (notes.textContent) card.appendChild(notes);
      column.appendChild(card);
    }});

    board.appendChild(column);
  }});
}}

function chip(text, accent=false) {{
  const span = document.createElement('span');
  span.className = 'chip' + (accent ? ' accent' : '');
  span.textContent = text;
  return span;
}}

[search, statusFilter, priorityFilter, groupBy].forEach(el => el.addEventListener('input', render));
render();
</script>
</body>
</html>
"""


def main():
    parser = argparse.ArgumentParser(description="Generate interactive roadmap dashboard")
    parser.add_argument("--input", default=".claude/system/features.json")
    parser.add_argument("--output", default="docs/roadmap.html")
    args = parser.parse_args()

    input_path = Path(args.input)
    output_path = Path(args.output)

    features = load_features(input_path)
    html = generate_html(features)
    output_path.write_text(html)
    print(f"Wrote roadmap to {output_path}")


if __name__ == "__main__":
    main()
