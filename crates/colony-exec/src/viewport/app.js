// The reports viewport. Report text comes from a model: every node is built with
// textContent, never as HTML.
"use strict";

const REFRESH_MS = 10000;
const LABELS = { fine: "Fine", attention: "Attention", blocked: "Blocked", needs_you: "Needs you" };

function el(tag, attrs, text) {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs || {})) node.setAttribute(k, v);
  if (text !== undefined) node.textContent = text;
  return node;
}

function when(ms) {
  return new Date(ms).toLocaleString([], { month: "short", day: "numeric", hour: "2-digit", minute: "2-digit" });
}

function item(report) {
  const li = el("li", { class: report.status });
  const meta = el("div", { class: "meta" });
  meta.append(
    el("span", { class: "status" }, LABELS[report.status] || report.status),
    el("span", {}, report.agent + " · run " + report.run),
    el("time", { datetime: new Date(report.at).toISOString() }, when(report.at)),
  );
  li.append(meta, el("div", { class: "headline" }, report.headline));
  if (report.body && report.body !== report.headline) {
    const details = el("details");
    details.append(el("summary", {}, "Details"), el("pre", {}, report.body));
    li.append(details);
  }
  if (report.evidence.length) {
    const ev = el("div", { class: "evidence" });
    for (const addr of report.evidence) {
      ev.append(el("a", { href: "/open?a=" + encodeURIComponent(addr), target: "_blank", rel: "noopener" }, addr));
    }
    li.append(ev);
  }
  return li;
}

function fill(list, reports, empty) {
  list.replaceChildren(...(reports.length ? reports.map(item) : [el("li", { class: "empty" }, empty)]));
}

async function refresh() {
  try {
    const resp = await fetch("/api/feed", { cache: "no-store" });
    if (!resp.ok) throw new Error("HTTP " + resp.status);
    const data = await resp.json();
    fill(document.getElementById("needs-you"), data.needs_you, "Nothing needs you.");
    fill(document.getElementById("feed"), data.feed, "No reports yet.");
    document.getElementById("updated").textContent = "Updated " + when(Date.now());
  } catch (err) {
    document.getElementById("updated").textContent = "Cannot reach the daemon (" + err.message + ")";
  }
}

refresh();
setInterval(refresh, REFRESH_MS);
