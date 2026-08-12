"""
Bondhive Dashboard — minimal HTTP server serving the live-data variant of
web/index.html. No build step, no external dependencies (stdlib only).

Status: planned. Every /api/* handler below returns example data shaped
exactly like the real response will be once wired to the Solana program +
oracle ledger (see the TODO in each handler) — this is not live data.
Do not read a green response here as "on-chain state."

Pattern ported from PROOF's dashboard.py (github.com/cryptonomicsed-byte/Proof,
now archived into this repo) as part of the round-2 orchestration decision to
fold PROOF's dashboard pattern into Bondhive: a static-mockup dashboard
(the original web/index.html) has no failure mode to catch drift between the
page and the protocol; a live-fetch dashboard against real endpoints does,
even while the endpoints return mock data pending real wiring.

Endpoints:
  GET /                        — the dashboard (glass/bento UI, same design
                                  language as web/index.html)
  GET /api/stats                — pool/supply/burn/staked totals
  GET /api/score/leaderboard    — top pubkeys by BondScore
  GET /api/bonds                — active + recently resolved bonds
  GET /api/attestations         — recent kind 37002 attestations
"""
import json
import time
from http.server import HTTPServer, BaseHTTPRequestHandler
from urllib.parse import urlparse


def get_stats():
    # TODO: replace with a real read of the PoolAccount PDA (see
    # program/src/state.rs::PoolAccount) once a program is deployed.
    return {
        "supply": 1_000_000_000,
        "burned_total": 12_400_000,
        "bonds_active": 384,
        "escrow": 2_940_000,
        "staked_total": 84_200_000,
        "milestone_progress_pct": 62,
        "generated_at": int(time.time()),
    }


def get_leaderboard():
    # TODO: replace with real NIP-32 kind 1985 "bondhive" label reads from
    # the Buzz relay, scored via bondhive-core::score.
    return {
        "leaders": [
            {"pubkey": "nsec…f1", "score": 84, "tier": "sovereign", "resolved": 41, "failed": 2},
            {"pubkey": "nsec…9d", "score": 71, "tier": "trusted", "resolved": 19, "failed": 3},
            {"pubkey": "nsec…72", "score": 63, "tier": "trusted", "resolved": 12, "failed": 4},
        ],
        "generated_at": int(time.time()),
    }


def get_bonds():
    # TODO: replace with a relay query for kind 37000-37005 events, joined
    # against BondAccount state for economics (see program/src/lib.rs).
    return {
        "bonds": [
            {"id": "9f3a…c21e", "agent": "nsec…f1", "channel": "#engineering", "amount": 10_000,
             "attest": "3/3", "outcome": "pass", "economics": "release 10,000 · witness fee 50"},
            {"id": "77d0…a9b2", "agent": "nsec…04", "channel": "#research", "amount": 25_000,
             "attest": "3/3", "outcome": "fail",
             "economics": "claimant 17,500 · witnesses 5,000 · burn 2,500"},
            {"id": "e1b8…f07c", "agent": "nsec…9d", "channel": "#infra", "amount": 5_000,
             "attest": "2/3", "outcome": "pending", "economics": "awaiting 1 witness"},
        ],
        "generated_at": int(time.time()),
    }


def get_attestations():
    # TODO: replace with a real relay subscription to kind 37002, filtered
    # by #h channel scope per docs/buzz-integration.md.
    return {
        "attestations": [
            {"bond_id": "9f3a…c21e", "witness": "nsec…22", "verdict": "pass", "ts": int(time.time()) - 120},
            {"bond_id": "e1b8…f07c", "witness": "nsec…31", "verdict": "pass", "ts": int(time.time()) - 340},
        ],
        "generated_at": int(time.time()),
    }


API_ROUTES = {
    "/api/stats": get_stats,
    "/api/score/leaderboard": get_leaderboard,
    "/api/bonds": get_bonds,
    "/api/attestations": get_attestations,
}

DASHBOARD_HTML = """<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Bondhive — live dashboard</title>
<style>
  :root { --bg:#07090f; --glass:rgba(255,255,255,0.06); --glass-strong:rgba(255,255,255,0.10);
    --stroke:rgba(255,255,255,0.14); --text:#e8ecf4; --muted:#8b93a7; --accent:#6ee7b7;
    --accent2:#7dd3fc; --gold:#fde68a; --bad:#fb7185; --radius:20px; }
  * { margin:0; padding:0; box-sizing:border-box; }
  body { background: radial-gradient(1200px 600px at 20% -10%, #12222e 0%, var(--bg) 55%),
      radial-gradient(900px 500px at 90% 110%, #1c1428 0%, var(--bg) 60%);
    color:var(--text); font-family: ui-sans-serif, system-ui, -apple-system, sans-serif; min-height:100vh; }
  .wrap { max-width:1100px; margin:0 auto; padding:40px 24px 80px; }
  h1 { font-size:clamp(24px,4vw,36px); margin-bottom:6px; }
  .tag { color:var(--muted); margin-bottom:28px; font-size:14px; }
  .grid { display:grid; grid-template-columns:repeat(12,1fr); gap:16px; }
  .card { border-radius:var(--radius); padding:22px; background:var(--glass); border:1px solid var(--stroke);
    backdrop-filter: blur(18px); }
  .k { font-size:11px; text-transform:uppercase; letter-spacing:0.14em; color:var(--muted); margin-bottom:10px; }
  .big { font-size:clamp(28px,4vw,40px); font-weight:700; font-variant-numeric: tabular-nums; }
  .span-3 { grid-column: span 3; } .span-6 { grid-column: span 6; } .span-12 { grid-column: span 12; }
  table { width:100%; border-collapse:collapse; font-size:13px; }
  th { text-align:left; color:var(--muted); font-weight:500; padding:8px 10px; border-bottom:1px solid var(--stroke); }
  td { padding:9px 10px; border-bottom:1px solid rgba(255,255,255,0.05); font-variant-numeric: tabular-nums; }
  .pill { font-size:11px; padding:3px 10px; border-radius:999px; font-weight:600; }
  .pass { background: rgba(110,231,183,0.15); color: var(--accent); }
  .fail { background: rgba(251,113,133,0.15); color: var(--bad); }
  .pend { background: rgba(253,230,138,0.15); color: var(--gold); }
  .loading { color: var(--muted); padding: 12px 0; }
  .foot { margin-top:30px; color:var(--muted); font-size:12px; text-align:center; }
  @media (max-width:760px){ .span-3,.span-6 { grid-column: span 12; } }
</style>
</head>
<body>
<div class="wrap">
  <h1>Bondhive — live dashboard</h1>
  <p class="tag">Every stat below is fetched from /api/* — mock data until the program is deployed. See dashboard.py TODOs for the real wiring points.</p>
  <div class="grid" id="stats-grid"><div class="loading">loading /api/stats…</div></div>
  <div class="grid" style="margin-top:16px">
    <div class="card span-6"><div class="k">BondScore leaderboard</div><div id="leaderboard"><div class="loading">loading…</div></div></div>
    <div class="card span-6"><div class="k">Recent attestations</div><div id="attestations"><div class="loading">loading…</div></div></div>
  </div>
  <div class="grid" style="margin-top:16px">
    <div class="card span-12"><div class="k">Bonds</div><div id="bonds"><div class="loading">loading…</div></div></div>
  </div>
  <div class="foot">Bondhive · design/planned — see README status table</div>
</div>
<script>
async function j(url) { const r = await fetch(url); return r.json(); }
function pill(o) { const cls = o === "pass" ? "pass" : o === "fail" ? "fail" : "pend"; return `<span class="pill ${cls}">${o}</span>`; }

async function loadStats() {
  const s = await j("/api/stats");
  document.getElementById("stats-grid").innerHTML = `
    <div class="card span-3"><div class="k">Supply · $BOND</div><div class="big">${s.supply.toLocaleString()}</div></div>
    <div class="card span-3"><div class="k">Burned total</div><div class="big">${s.burned_total.toLocaleString()}</div></div>
    <div class="card span-3"><div class="k">Active bonds</div><div class="big">${s.bonds_active}</div></div>
    <div class="card span-3"><div class="k">Staked in pool</div><div class="big">${s.staked_total.toLocaleString()}</div></div>`;
}
async function loadLeaderboard() {
  const { leaders } = await j("/api/score/leaderboard");
  document.getElementById("leaderboard").innerHTML = `<table><tr><th>pubkey</th><th>score</th><th>tier</th></tr>` +
    leaders.map(l => `<tr><td>${l.pubkey}</td><td>${l.score}</td><td>${l.tier}</td></tr>`).join("") + `</table>`;
}
async function loadAttestations() {
  const { attestations } = await j("/api/attestations");
  document.getElementById("attestations").innerHTML = `<table><tr><th>bond</th><th>witness</th><th>verdict</th></tr>` +
    attestations.map(a => `<tr><td>${a.bond_id}</td><td>${a.witness}</td><td>${pill(a.verdict)}</td></tr>`).join("") + `</table>`;
}
async function loadBonds() {
  const { bonds } = await j("/api/bonds");
  document.getElementById("bonds").innerHTML = `<table><tr><th>bond</th><th>agent</th><th>channel</th><th>amount</th><th>attest</th><th>outcome</th><th>economics</th></tr>` +
    bonds.map(b => `<tr><td>${b.id}</td><td>${b.agent}</td><td>${b.channel}</td><td>${b.amount.toLocaleString()}</td><td>${b.attest}</td><td>${pill(b.outcome)}</td><td>${b.economics}</td></tr>`).join("") + `</table>`;
}
Promise.all([loadStats(), loadLeaderboard(), loadAttestations(), loadBonds()]).catch(e => console.error(e));
</script>
</body>
</html>"""


class Handler(BaseHTTPRequestHandler):
    def do_GET(self):
        path = urlparse(self.path).path
        if path in API_ROUTES:
            body = json.dumps(API_ROUTES[path]()).encode()
            self.send_response(200)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        if path == "/":
            body = DASHBOARD_HTML.encode()
            self.send_response(200)
            self.send_header("Content-Type", "text/html")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)
            return
        self.send_response(404)
        self.end_headers()

    def log_message(self, fmt, *args):
        pass  # quiet by default; flip to super().log_message for debugging


if __name__ == "__main__":
    import sys
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8420
    HTTPServer(("127.0.0.1", port), Handler).serve_forever()
