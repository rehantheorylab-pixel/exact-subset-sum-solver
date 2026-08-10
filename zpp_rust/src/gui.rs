use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::settings::EngineSettings;
use crate::{pick_engines, race, Engine, reduce};
use num_traits::Zero;

pub fn serve(port: u16) {
    let addr = format!("127.0.0.1:{}", port);
    let listener = match TcpListener::bind(&addr) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Failed to start GUI server: {}", e);
            return;
        }
    };
    println!("\n  Algorithm GUI started at: http://{}\n  Open this in your browser.", addr);
    for stream in listener.incoming() {
        match stream {
            Ok(s) => { std::thread::spawn(|| handle(s)); },
            Err(_) => break,
        }
    }
}

fn handle(mut stream: TcpStream) {
    let mut buf = Vec::new();
    let mut reader = BufReader::new(&stream);
    reader.read_until(b'\n', &mut buf).ok();
    let first = String::from_utf8_lossy(&buf);
    let parts: Vec<&str> = first.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];

    let mut content_length = 0usize;
    loop {
        let mut hbuf = Vec::new();
        if reader.read_until(b'\n', &mut hbuf).ok() == Some(0) {
            break;
        }
        if hbuf.len() <= 2 {
            break; // blank line (\r\n or \n) — end of headers
        }
        let hdr = String::from_utf8_lossy(&hbuf);
        if hdr.to_lowercase().starts_with("content-length:") {
            if let Ok(n) = hdr["content-length:".len()..].trim().parse::<usize>() {
                content_length = n;
            }
        }
    }

    // Read body if present
    let mut body = Vec::new();
    if content_length > 0 {
        reader.take(content_length as u64).read_to_end(&mut body).ok();
    }

    let (status, resp_body) = route(method, path, &body);
    let resp = format!(
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
        status,
        if path == "/" || path.starts_with("/static/") { "text/html; charset=utf-8" } else { "application/json; charset=utf-8" },
        resp_body.len(),
        resp_body,
    );
    let _ = stream.write_all(resp.as_bytes());
    let _ = stream.flush();
}

fn route(method: &str, path: &str, body: &[u8]) -> (&'static str, String) {
    match (method, path) {
        ("GET", "/") => ("200 OK", String::from(HTML)),
        ("GET", "/api/settings") => {
            let s = EngineSettings::load();
            ("200 OK", serde_json::to_string(&s).unwrap_or_default())
        }
        ("POST", "/api/settings") => {
            if let Ok(s) = serde_json::from_slice::<EngineSettings>(body) {
                s.save();
                ("200 OK", r#"{"ok":true}"#.to_string())
            } else {
                ("400 Bad Request", r#"{"error":"invalid settings"}"#.to_string())
            }
        }
        ("POST", "/api/solve") => {
            handle_solve(body)
        }
        ("GET", "/api/engines") => {
            let names = crate::scheduler::all_engine_names();
            ("200 OK", serde_json::to_string(&names).unwrap_or_default())
        }
        _ => ("404 Not Found", String::new()),
    }
}

fn handle_solve(body: &[u8]) -> (&'static str, String) {
    #[derive(serde::Deserialize)]
    struct SolveReq {
        numbers: String,
        target: String,
        timeout: Option<u64>,
    }
    let req: SolveReq = match serde_json::from_slice(body) {
        Ok(r) => r,
        Err(e) => return ("400 Bad Request", format!(r#"{{"error":"{}"}}"#, e)),
    };

    let nums: Vec<num_bigint::BigUint> = req.numbers
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter(|s| !s.is_empty())
        .filter_map(|s| num_bigint::BigUint::parse_bytes(s.as_bytes(), 10))
        .collect();

    let target = match num_bigint::BigUint::parse_bytes(req.target.as_bytes(), 10) {
        Some(t) => t,
        None => return ("400 Bad Request", r#"{"error":"invalid target"}"#.to_string()),
    };

    if nums.is_empty() {
        if target.is_zero() {
            return ("200 OK", r#"{"result":"solved","winner":"Preprocessor","solution":"","time_ns":0}"#.to_string());
        }
        return ("200 OK", r#"{"result":"impossible","winner":"Preprocessor","time_ns":0}"#.to_string());
    }

    let timeout = Duration::from_secs(req.timeout.unwrap_or(300));
    let red = reduce(&nums, &target);
    if red.impossible {
        return ("200 OK", r#"{"result":"impossible","winner":"Preprocessor","time_ns":0}"#.to_string());
    }
    if red.target.is_zero() {
        let sol_str = red.forced.iter()
            .map(|x| x.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        return ("200 OK", format!(r#"{{"result":"solved","winner":"Preprocessor","solution":"{}","time_ns":0}}"#, sol_str));
    }

    let red_profile = crate::profile::Profile::new(red.numbers.clone(), red.target.clone());
    let hw = crate::hardware_profile::HardwareProfile::detect();
    let start = std::time::Instant::now();

    // Apply engine settings
    let settings = EngineSettings::load();
    let mut names = pick_engines(&red_profile, &hw);
    names.retain(|n| *settings.enabled.get(*n).unwrap_or(&true));
    names.sort_by(|a, b| {
        let pa = settings.priority.get(*a).copied().unwrap_or(50.0);
        let pb = settings.priority.get(*b).copied().unwrap_or(50.0);
        pb.partial_cmp(&pa).unwrap_or(std::cmp::Ordering::Equal)
    });
    if names.len() > settings.max_engines {
        names.truncate(settings.max_engines);
    }

    // Safety cap: prevent crashes. Keep solver engines for large n.
    if red_profile.n >= 40 {
        let safe = if red_profile.n >= 60 { 4 } else { 6 };
        // Always keep solver engines
        let mut kept: Vec<&str> = Vec::new();
        let mut rest: Vec<&str> = Vec::new();
        for n in &names {
            if matches!(*n, "GroupDecompose" | "Schroeppel-Shamir") {
                if kept.len() < 2 { kept.push(n); }
            } else {
                rest.push(n);
            }
        }
        kept.extend(rest.into_iter().take(safe - kept.len()));
        if kept.len() < names.len() { names = kept; }
    }

    let engines: Vec<Box<dyn Engine>> = names.iter()
        .filter_map(|n| crate::engines::build(n))
        .collect();

    let mut out = race(red_profile.clone(), engines, timeout);
    if out.solution.is_none() && !out.proved_impossible {
        let learn = crate::learning::LearningStore::load();
        let names2 = learn.bias_order(&red_profile, pick_engines(&red_profile, &hw));
        let engines2: Vec<Box<dyn Engine>> = names2.iter()
            .filter_map(|n| crate::engines::build(n))
            .collect();
        out = race(red_profile.clone(), engines2, timeout);
    }

    if out.winner != "Timeout" && out.winner != "IMPOSSIBLE" {
        crate::learning::LearningStore::load().record_win(&red_profile, out.winner);
    }
    if let Some(sol) = out.solution.as_mut() {
        let mut full: Vec<num_bigint::BigUint> = red.forced.clone();
        full.extend(sol.iter().cloned());
        *sol = full;
    }

    let elapsed_ns = start.elapsed().as_nanos();

    let result = if out.solution.is_some() {
        "solved"
    } else if out.proved_impossible {
        "impossible"
    } else {
        "timeout"
    };

    let sol_str = out.solution.as_ref().map(|sol| {
        sol.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(", ")
    }).unwrap_or_default();

    ("200 OK", format!(r#"{{"result":"{}","winner":"{}","solution":"{}","time_ns":{}}}"#, result, out.winner, sol_str, elapsed_ns))
}

const HTML: &str = r###"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Algorithm — Subset Sum Solver</title>
<style>
*{margin:0;padding:0;box-sizing:border-box}
body{font-family:'Segoe UI',system-ui,sans-serif;background:#0f0f1a;color:#e0e0f0;min-height:100vh}
header{background:linear-gradient(135deg,#1a1a3e,#2a1a4e);padding:20px 40px;border-bottom:2px solid #4a3a8a}
header h1{font-size:24px;color:#b8a0ff}
header span{color:#888;font-size:14px}
nav{display:flex;gap:2px;background:#1a1a2e;padding:0 40px}
nav button{padding:12px 24px;border:none;background:#1a1a2e;color:#888;cursor:pointer;font-size:14px;border-bottom:2px solid transparent;transition:all .2s}
nav button:hover{color:#e0e0f0;background:#2a2a4e}
nav button.active{color:#b8a0ff;border-bottom-color:#b8a0ff;background:#2a2a4e}
.content{display:none;padding:30px 40px;max-width:900px}
.content.active{display:block}
.card{background:#1a1a2e;border:1px solid #2a2a4e;border-radius:8px;padding:20px;margin-bottom:20px}
.card h3{color:#b8a0ff;margin-bottom:12px;font-size:16px}
label{display:block;color:#aaa;margin-bottom:4px;font-size:13px}
textarea,input[type=text]{width:100%;background:#0f0f1a;border:1px solid #2a2a4e;color:#e0e0f0;padding:10px 12px;border-radius:6px;font-family:'Consolas','Courier New',monospace;font-size:13px;margin-bottom:12px}
textarea:focus,input:focus{outline:none;border-color:#6a5acd}
textarea{min-height:120px;resize:vertical}
.btn{padding:10px 24px;border:none;border-radius:6px;cursor:pointer;font-size:14px;transition:all .2s}
.btn-primary{background:#6a5acd;color:#fff}
.btn-primary:hover{background:#7b6bde}
.btn-secondary{background:#2a2a4e;color:#e0e0f0}
.btn-secondary:hover{background:#3a3a5e}
.btn-danger{background:#8a2a2a;color:#fff}
.btn-danger:hover{background:#aa3a3a}
.file-upload{border:2px dashed #2a2a4e;border-radius:8px;padding:40px;text-align:center;cursor:pointer;transition:all .2s;margin-bottom:12px}
.file-upload:hover{border-color:#6a5acd;background:#1a1a2e}
.file-upload input{display:none}
.engine-row{display:flex;align-items:center;gap:12px;padding:8px 12px;border-bottom:1px solid #1a1a2e}
.engine-row:hover{background:#1a1a2e}
.engine-row .name{flex:1;font-size:13px}
.engine-row .toggle{position:relative;width:40px;height:22px;cursor:pointer}
.engine-row .toggle input{display:none}
.engine-row .toggle .slider{position:absolute;inset:0;background:#2a2a4e;border-radius:11px;transition:.2s}
.engine-row .toggle .slider::before{content:'';position:absolute;width:18px;height:18px;left:2px;bottom:2px;background:#666;border-radius:50%;transition:.2s}
.engine-row .toggle input:checked+.slider{background:#6a5acd}
.engine-row .toggle input:checked+.slider::before{background:#fff;transform:translateX(18px)}
.engine-row input[type=range]{width:120px;accent-color:#6a5acd}
.result-box{background:#0f0f1a;border:1px solid #2a2a4e;border-radius:6px;padding:16px;font-family:Consolas,monospace;font-size:13px;white-space:pre-wrap;min-height:60px;margin-top:12px}
.result-box .solved{color:#4caf50}
.result-box .impossible{color:#f44336}
.result-box .timeout{color:#ff9800}
.status-bar{display:flex;gap:16px;padding:8px 40px;background:#1a1a2e;font-size:12px;color:#666;border-top:1px solid #2a2a4e}
#fileStatus{color:#aaa;font-size:12px;margin-top:8px}
.loading{display:inline-block;width:16px;height:16px;border:2px solid #6a5acd;border-top-color:transparent;border-radius:50%;animation:spin .8s linear infinite;vertical-align:middle;margin-right:8px}
@keyframes spin{to{transform:rotate(360deg)}}
.settings-grid{display:grid;grid-template-columns:1fr 1fr;gap:16px}
@media(max-width:600px){.settings-grid{grid-template-columns:1fr}}
</style>
</head>
<body>
<header><h1>Algorithm — Subset Sum Solver</h1><span>Z++ Ultimate Engine v1.1</span></header>
<nav>
<button class="active" onclick="switchTab('input')">Input</button>
<button onclick="switchTab('engines')">Engines</button>
<button onclick="switchTab('settings')">Settings</button>
</nav>

<div id="tab-input" class="content active">
<div class="card">
<h3>Enter Numbers</h3>
<label>Numbers (comma or space separated)</label>
<textarea id="numbersInput" placeholder="e.g. 3, 5, 7, 11, 13, 17, 19, 23, 29, 31"></textarea>
<label>Target Sum</label>
<input type="text" id="targetInput" placeholder="e.g. 55">
</div>
<div class="card">
<h3>Or Upload File</h3>
<div class="file-upload" id="fileDrop" onclick="document.getElementById('fileInput').click()">
<input type="file" id="fileInput" accept=".txt,.csv">
<span>Drop a .txt or .csv file here, or click to browse</span>
<div id="fileStatus"></div>
</div>
</div>
<button class="btn btn-primary" onclick="solve()" id="solveBtn">Run Algorithm</button>
<div class="result-box" id="resultBox">Results will appear here.</div>
</div>

<div id="tab-engines" class="content">
<div class="card">
<h3>Engine Configuration</h3>
<p style="color:#888;font-size:13px;margin-bottom:16px">Toggle engines on/off and adjust priority (higher = runs first).</p>
<div id="engineList"></div>
</div>
<button class="btn btn-primary" onclick="saveEngineSettings()">Save Engine Settings</button>
<span id="engineSaveStatus" style="margin-left:12px;color:#4caf50;font-size:13px"></span>
</div>

<div id="tab-settings" class="content">
<div class="card">
<h3>General Settings</h3>
<div class="settings-grid">
<div>
<label>Timeout (seconds)</label>
<input type="text" id="timeoutInput" value="300">
</div>
<div>
<label>Max Engines</label>
<input type="text" id="maxEnginesInput" value="20">
</div>
</div>
<button class="btn btn-primary" onclick="saveGeneralSettings()" style="margin-top:12px">Save Settings</button>
<span id="settingsSaveStatus" style="margin-left:12px;color:#4caf50;font-size:13px"></span>
</div>
</div>

<div class="status-bar">
<span id="engineCount">Loading...</span>
<span id="lastSolve"></span>
</div>

<script>
let allEngines = [];

async function loadSettings() {
    const r = await fetch('/api/settings');
    const s = await r.json();
    document.getElementById('timeoutInput').value = s.timeout_secs || 300;
    document.getElementById('maxEnginesInput').value = s.max_engines || 20;
    return s;
}

async function loadEngines() {
    const r = await fetch('/api/engines');
    allEngines = await r.json();
    const s = await loadSettings();
    const list = document.getElementById('engineList');
    list.innerHTML = '';
    for (const name of allEngines) {
        const en = s.enabled[name] !== false;
        const pri = s.priority[name] || 50;
        const row = document.createElement('div');
        row.className = 'engine-row';
        row.innerHTML = `
            <label class="toggle">
                <input type="checkbox" ${en?'checked':''} data-engine="${name}" onchange="markEngineDirty()">
                <span class="slider"></span>
            </label>
            <span class="name">${name}</span>
            <input type="range" min="0" max="100" value="${pri}" data-engine="${name}" oninput="markEngineDirty()">
            <span style="font-size:12px;color:#888;width:30px">${pri}</span>
        `;
        list.appendChild(row);
    }
    document.getElementById('engineCount').textContent = `${allEngines.length} engines available`;
}

let engineDirty = false;
function markEngineDirty() {
    engineDirty = true;
    document.getElementById('engineSaveStatus').textContent = '';
    // Update priority display
    event.target.parentElement.querySelector('span:last-child').textContent = event.target.value;
}

function collectEngineSettings() {
    const enabled = {}, priority = {};
    document.querySelectorAll('.engine-row').forEach(row => {
        const cb = row.querySelector('input[type=checkbox]');
        const range = row.querySelector('input[type=range]');
        const name = cb.dataset.engine;
        enabled[name] = cb.checked;
        priority[name] = parseFloat(range.value);
    });
    const s = {
        enabled, priority,
        timeout_secs: parseInt(document.getElementById('timeoutInput').value) || 300,
        max_engines: parseInt(document.getElementById('maxEnginesInput').value) || 20,
    };
    return s;
}

async function saveEngineSettings() {
    const s = collectEngineSettings();
    const r = await fetch('/api/settings', {
        method:'POST', headers:{'Content-Type':'application/json'},
        body: JSON.stringify(s),
    });
    const j = await r.json();
    document.getElementById('engineSaveStatus').textContent = j.ok ? 'Saved!' : 'Error saving';
    engineDirty = false;
}

async function saveGeneralSettings() {
    const s = collectEngineSettings();
    const r = await fetch('/api/settings', {
        method:'POST', headers:{'Content-Type':'application/json'},
        body: JSON.stringify(s),
    });
    const j = await r.json();
    document.getElementById('settingsSaveStatus').textContent = j.ok ? 'Saved!' : 'Error saving';
}

function switchTab(name) {
    document.querySelectorAll('.content').forEach(c => c.classList.remove('active'));
    document.querySelectorAll('nav button').forEach(b => b.classList.remove('active'));
    document.getElementById('tab-' + name).classList.add('active');
    event.target.classList.add('active');
    if (name === 'engines' && engineDirty) {
        // reload engines to reset
    }
}

async function solve() {
    const btn = document.getElementById('solveBtn');
    btn.disabled = true;
    btn.innerHTML = '<span class="loading"></span> Running...';
    document.getElementById('resultBox').innerHTML = 'Running algorithm...';

    const numbers = document.getElementById('numbersInput').value.trim();
    const target = document.getElementById('targetInput').value.trim();

    if (!numbers || !target) {
        document.getElementById('resultBox').innerHTML = '<span class="timeout">Please enter numbers and a target.</span>';
        btn.disabled = false;
        btn.textContent = 'Run Algorithm';
        return;
    }

    try {
        const r = await fetch('/api/solve', {
            method:'POST', headers:{'Content-Type':'application/json'},
            body: JSON.stringify({
                numbers, target,
                timeout: parseInt(document.getElementById('timeoutInput').value) || 300,
            }),
        });
        const j = await r.json();
        let html = '';
        if (j.result === 'solved') {
            html += '<span class="solved">✓ Solved!</span>\n';
            html += 'Winner: ' + j.winner + '\n';
            if (j.solution) html += 'Solution: [' + j.solution + ']\n';
        } else if (j.result === 'impossible') {
            html += '<span class="impossible">✗ Proved Impossible</span>\n';
            html += 'Winner: ' + j.winner + '\n';
        } else {
            html += '<span class="timeout">⏱ Timeout</span>\n';
            html += 'Winner: ' + j.winner + '\n';
        }
        html += 'Time: ' + (j.time_ns / 1_000_000).toFixed(2) + ' ms';
        document.getElementById('resultBox').innerHTML = html;
        document.getElementById('lastSolve').textContent = 'Last: ' + j.result + ' (' + (j.time_ns / 1_000_000).toFixed(1) + 'ms)';
    } catch(e) {
        document.getElementById('resultBox').innerHTML = '<span class="timeout">Error: ' + e.message + '</span>';
    }
    btn.disabled = false;
    btn.textContent = 'Run Algorithm';
}

// File upload handling
document.getElementById('fileInput').addEventListener('change', function(e) {
    const file = e.target.files[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = function(ev) {
        const text = ev.target.result;
        const lines = text.split('\n').filter(l => l.trim());
        // Look for "goal:" marker
        let nums = '', target = '';
        for (const line of lines) {
            if (line.toLowerCase().startsWith('goal:') || line.toLowerCase().startsWith('target:')) {
                target = line.replace(/^[^:]+:\s*/, '').trim();
            } else if (line.includes(',')) {
                nums = line;
            } else if (nums) {
                target = line.trim();
            } else {
                nums = line;
            }
        }
        if (nums) document.getElementById('numbersInput').value = nums;
        if (target) document.getElementById('targetInput').value = target;
        document.getElementById('fileStatus').textContent = 'Loaded: ' + file.name;
    };
    reader.readAsText(file);
});

// Drag and drop
document.getElementById('fileDrop').addEventListener('dragover', function(e) {
    e.preventDefault();
    this.style.borderColor = '#6a5acd';
    this.style.background = '#1a1a2e';
});
document.getElementById('fileDrop').addEventListener('dragleave', function(e) {
    e.preventDefault();
    this.style.borderColor = '#2a2a4e';
    this.style.background = 'transparent';
});
document.getElementById('fileDrop').addEventListener('drop', function(e) {
    e.preventDefault();
    this.style.borderColor = '#2a2a4e';
    this.style.background = 'transparent';
    const file = e.dataTransfer.files[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = function(ev) {
        const text = ev.target.result;
        const lines = text.split('\n').filter(l => l.trim());
        let nums = '', target = '';
        for (const line of lines) {
            if (line.toLowerCase().startsWith('goal:') || line.toLowerCase().startsWith('target:')) {
                target = line.replace(/^[^:]+:\s*/, '').trim();
            } else if (line.includes(',')) {
                nums = line;
            } else if (nums) {
                target = line.trim();
            } else {
                nums = line;
            }
        }
        if (nums) document.getElementById('numbersInput').value = nums;
        if (target) document.getElementById('targetInput').value = target;
        document.getElementById('fileStatus').textContent = 'Loaded: ' + file.name;
    };
    reader.readAsText(file);
});

loadEngines();
</script>
</body>
</html>"###;
