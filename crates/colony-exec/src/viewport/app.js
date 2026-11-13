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

async function send(ask, answer, status) {
  status.textContent = "Sending\u2026";
  try {
    const resp = await fetch("/api/answer", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ask: ask.address, answer }),
    });
    if (!resp.ok) throw new Error(await resp.text());
    status.textContent = "Answered. The run goes on.";
    refresh(true);
  } catch (err) {
    status.textContent = "Not sent: " + err.message;
  }
}

function askNode(ask) {
  const box = el("div", { class: "ask" });
  box.append(el("div", { class: "question" }, ask.question));
  if (ask.answer !== null) {
    box.append(el("div", { class: "answered" },
      "Answered \u201c" + ask.answer + "\u201d by " + ask.by + ", " + when(ask.answered_at)));
    return box;
  }
  const status = el("div", { class: "send-status", "aria-live": "polite" });
  const row = el("div", { class: "answers" });
  if (ask.options.length) {
    for (const opt of ask.options) {
      const b = el("button", { type: "button" }, opt);
      b.addEventListener("click", () => send(ask, opt, status));
      row.append(b);
    }
  } else {
    const input = el("textarea", { rows: "2", "aria-label": "Your answer", maxlength: "2000" });
    const b = el("button", { type: "button" }, "Send");
    b.addEventListener("click", () => input.value.trim() && send(ask, input.value, status));
    row.append(input, b);
  }
  box.append(row, status);
  return box;
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
  for (const ask of report.asks || []) li.append(askNode(ask));
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

// Someone typing an answer: a refresh would wipe it.
function typing() {
  return [...document.querySelectorAll("textarea")].some((t) => t.value || t === document.activeElement);
}

async function refresh(force) {
  if (!force && typing()) return;
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
