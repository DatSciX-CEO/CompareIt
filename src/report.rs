//! HTML report generation
//!
//! This module generates self-contained HTML reports from comparison results,
//! featuring:
//! - Dashboard with pie chart visualization
//! - Sortable results table
//! - Embedded side-by-side diff viewer
//! - Structured data mismatch highlights

use crate::types::{ComparisonResult, ComparisonSummary};
use anyhow::{Context, Result};
use std::fs;
use std::path::Path;

/// Generate an HTML report from comparison results
pub fn generate_html_report(
    results: &[ComparisonResult],
    summary: &ComparisonSummary,
    output_path: &Path,
    artifacts_dir: Option<&Path>,
) -> Result<()> {
    let html = build_html_report(results, summary, artifacts_dir);

    fs::write(output_path, html)
        .with_context(|| format!("Failed to write HTML report to {}", output_path.display()))?;

    Ok(())
}

/// Build the HTML report content
fn build_html_report(
    results: &[ComparisonResult],
    summary: &ComparisonSummary,
    artifacts_dir: Option<&Path>,
) -> String {
    let mut html = String::new();

    // HTML header with embedded CSS and JS
    html.push_str(&build_html_head());

    // Body start
    html.push_str(r#"
<body>
    <div class="container">
        <header>
            <h1>CompareIt Report</h1>
            <p class="subtitle">File comparison analysis</p>
        </header>
"#);

    // Dashboard with pie chart
    html.push_str(&build_dashboard(summary));

    // Summary cards
    html.push_str(&build_summary_cards(summary));

    // Results table
    html.push_str(&build_results_table(results, artifacts_dir));

    // Diff modal
    html.push_str(&build_diff_modal());

    // Embedded diff data (JSON)
    html.push_str(&build_diff_data(results));

    // JavaScript
    html.push_str(&build_javascript());

    html.push_str(r#"
    </div>
</body>
</html>
"#);

    html
}

/// Build HTML head with styles
fn build_html_head() -> String {
    r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>CompareIt Report</title>
    <style>
        :root {
            --bg-primary: #0d1117;
            --bg-secondary: #161b22;
            --bg-tertiary: #21262d;
            --text-primary: #c9d1d9;
            --text-secondary: #8b949e;
            --accent: #58a6ff;
            --success: #3fb950;
            --warning: #d29922;
            --danger: #f85149;
            --border: #30363d;
            --diff-add-bg: rgba(63, 185, 80, 0.15);
            --diff-del-bg: rgba(248, 81, 73, 0.15);
        }
        
        * { box-sizing: border-box; margin: 0; padding: 0; }
        
        body {
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Noto Sans', Helvetica, Arial, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            line-height: 1.6;
            padding: 2rem;
        }
        
        .container { max-width: 1400px; margin: 0 auto; }
        
        header { margin-bottom: 2rem; }
        h1 { font-size: 2rem; font-weight: 600; color: var(--text-primary); }
        .subtitle { color: var(--text-secondary); }
        
        /* Dashboard */
        .dashboard {
            display: flex;
            gap: 2rem;
            margin-bottom: 2rem;
            flex-wrap: wrap;
        }
        
        .pie-container {
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 1.5rem;
            min-width: 280px;
        }
        
        .pie-container h3 {
            font-size: 0.875rem;
            color: var(--text-secondary);
            margin-bottom: 1rem;
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }
        
        .pie-chart {
            width: 200px;
            height: 200px;
            border-radius: 50%;
            margin: 0 auto 1rem;
        }
        
        .pie-legend {
            display: flex;
            flex-direction: column;
            gap: 0.5rem;
        }
        
        .legend-item {
            display: flex;
            align-items: center;
            gap: 0.5rem;
            font-size: 0.875rem;
        }
        
        .legend-dot {
            width: 12px;
            height: 12px;
            border-radius: 50%;
        }
        
        .legend-dot.identical { background: var(--success); }
        .legend-dot.different { background: var(--warning); }
        .legend-dot.error { background: var(--danger); }
        
        /* Summary Cards */
        .summary-grid {
            display: grid;
            grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
            gap: 1rem;
            flex: 1;
        }
        
        .summary-card {
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 8px;
            padding: 1rem;
        }
        
        .summary-card .label {
            font-size: 0.75rem;
            color: var(--text-secondary);
            text-transform: uppercase;
            letter-spacing: 0.05em;
        }
        
        .summary-card .value {
            font-size: 1.75rem;
            font-weight: 600;
            margin-top: 0.25rem;
        }
        
        .summary-card .value.success { color: var(--success); }
        .summary-card .value.warning { color: var(--warning); }
        .summary-card .value.danger { color: var(--danger); }
        
        /* Table */
        .table-container {
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 8px;
            overflow: hidden;
            margin-top: 2rem;
        }
        
        .table-header {
            padding: 1rem;
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        
        .table-header h2 { font-size: 1rem; font-weight: 600; }
        
        .filter-input {
            background: var(--bg-tertiary);
            border: 1px solid var(--border);
            border-radius: 4px;
            padding: 0.5rem 0.75rem;
            color: var(--text-primary);
            font-size: 0.875rem;
        }
        
        table { width: 100%; border-collapse: collapse; font-size: 0.875rem; }
        
        th, td {
            padding: 0.75rem 1rem;
            text-align: left;
            border-bottom: 1px solid var(--border);
        }
        
        th {
            background: var(--bg-tertiary);
            font-weight: 600;
            color: var(--text-secondary);
            cursor: pointer;
            user-select: none;
        }
        
        th:hover { color: var(--text-primary); }
        th.sorted-asc::after { content: ' ▲'; }
        th.sorted-desc::after { content: ' ▼'; }
        
        tr:hover { background: var(--bg-tertiary); }
        
        .badge {
            display: inline-block;
            padding: 0.125rem 0.5rem;
            border-radius: 9999px;
            font-size: 0.75rem;
            font-weight: 500;
        }
        
        .badge.identical { background: rgba(63, 185, 80, 0.2); color: var(--success); }
        .badge.different { background: rgba(210, 153, 34, 0.2); color: var(--warning); }
        .badge.error { background: rgba(248, 81, 73, 0.2); color: var(--danger); }
        
        .similarity-bar {
            width: 60px;
            height: 6px;
            background: var(--bg-tertiary);
            border-radius: 3px;
            overflow: hidden;
            display: inline-block;
            vertical-align: middle;
            margin-right: 0.5rem;
        }
        
        .similarity-bar .fill { height: 100%; border-radius: 3px; }
        .similarity-bar .fill.high { background: var(--success); }
        .similarity-bar .fill.medium { background: var(--warning); }
        .similarity-bar .fill.low { background: var(--danger); }
        
        .path {
            max-width: 250px;
            overflow: hidden;
            text-overflow: ellipsis;
            white-space: nowrap;
            font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
            font-size: 0.8125rem;
        }
        
        .btn {
            background: var(--accent);
            color: var(--bg-primary);
            border: none;
            border-radius: 4px;
            padding: 0.25rem 0.5rem;
            font-size: 0.75rem;
            cursor: pointer;
            font-weight: 500;
        }
        
        .btn:hover { opacity: 0.9; }
        .btn:disabled { opacity: 0.5; cursor: not-allowed; }
        
        /* Modal */
        .modal-overlay {
            display: none;
            position: fixed;
            top: 0;
            left: 0;
            right: 0;
            bottom: 0;
            background: rgba(0, 0, 0, 0.8);
            z-index: 1000;
            overflow-y: auto;
            padding: 2rem;
        }
        
        .modal-overlay.active { display: flex; justify-content: center; }
        
        .modal {
            background: var(--bg-secondary);
            border: 1px solid var(--border);
            border-radius: 8px;
            width: 100%;
            max-width: 1200px;
            max-height: 90vh;
            display: flex;
            flex-direction: column;
        }
        
        .modal-header {
            padding: 1rem;
            border-bottom: 1px solid var(--border);
            display: flex;
            justify-content: space-between;
            align-items: center;
        }
        
        .modal-header h3 { font-size: 1rem; }
        
        .modal-close {
            background: transparent;
            border: none;
            color: var(--text-secondary);
            font-size: 1.5rem;
            cursor: pointer;
            line-height: 1;
        }
        
        .modal-close:hover { color: var(--text-primary); }
        
        .modal-body {
            padding: 1rem;
            overflow-y: auto;
            flex: 1;
        }
        
        /* Diff View */
        .diff-container {
            display: grid;
            grid-template-columns: 1fr 1fr;
            gap: 1rem;
            font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace;
            font-size: 0.8125rem;
        }
        
        .diff-panel {
            background: var(--bg-tertiary);
            border-radius: 4px;
            overflow: hidden;
        }
        
        .diff-panel-header {
            padding: 0.5rem 1rem;
            background: var(--bg-primary);
            font-weight: 600;
            font-size: 0.75rem;
            color: var(--text-secondary);
        }
        
        .diff-content {
            padding: 0.5rem 0;
            max-height: 500px;
            overflow-y: auto;
        }
        
        .diff-line {
            display: flex;
            min-height: 1.4em;
            padding: 0 1rem;
        }
        
        .diff-line-num {
            width: 40px;
            color: var(--text-secondary);
            text-align: right;
            padding-right: 1rem;
            user-select: none;
        }
        
        .diff-line-content {
            flex: 1;
            white-space: pre-wrap;
            word-break: break-all;
        }
        
        .diff-line.added { background: var(--diff-add-bg); }
        .diff-line.removed { background: var(--diff-del-bg); }
        
        /* Structured Diff */
        .struct-diff {
            overflow-x: auto;
        }
        
        .struct-diff table {
            min-width: 100%;
        }
        
        .struct-diff th {
            position: sticky;
            top: 0;
            background: var(--bg-tertiary);
        }
        
        .cell-mismatch {
            background: rgba(248, 81, 73, 0.2);
            color: var(--danger);
        }
        
        a { color: var(--accent); text-decoration: none; }
        a:hover { text-decoration: underline; }
    </style>
</head>
"#.to_string()
}

/// Build dashboard with pie chart
fn build_dashboard(summary: &ComparisonSummary) -> String {
    let total = summary.pairs_compared.max(1) as f64;
    let identical_pct = (summary.identical_pairs as f64 / total * 100.0).round();
    let different_pct = (summary.different_pairs as f64 / total * 100.0).round();
    let error_pct = (summary.error_pairs as f64 / total * 100.0).round();

    // Calculate pie chart angles (CSS conic-gradient)
    let identical_deg = identical_pct * 3.6;
    let different_deg = different_pct * 3.6;
    // error_deg is implicit (fills to 360)

    format!(r#"
        <div class="dashboard">
            <div class="pie-container">
                <h3>Status Distribution</h3>
                <div class="pie-chart" style="background: conic-gradient(
                    var(--success) 0deg {identical_deg}deg,
                    var(--warning) {identical_deg}deg {}deg,
                    var(--danger) {}deg 360deg
                );"></div>
                <div class="pie-legend">
                    <div class="legend-item">
                        <span class="legend-dot identical"></span>
                        <span>Identical ({} - {:.0}%)</span>
                    </div>
                    <div class="legend-item">
                        <span class="legend-dot different"></span>
                        <span>Different ({} - {:.0}%)</span>
                    </div>
                    <div class="legend-item">
                        <span class="legend-dot error"></span>
                        <span>Errors ({} - {:.0}%)</span>
                    </div>
                </div>
            </div>
"#,
        identical_deg + different_deg,
        identical_deg + different_deg,
        summary.identical_pairs, identical_pct,
        summary.different_pairs, different_pct,
        summary.error_pairs, error_pct
    )
}

/// Build summary cards
fn build_summary_cards(summary: &ComparisonSummary) -> String {
    format!(r#"
            <div class="summary-grid">
                <div class="summary-card">
                    <div class="label">Pairs Compared</div>
                    <div class="value">{}</div>
                </div>
                <div class="summary-card">
                    <div class="label">Identical</div>
                    <div class="value success">{}</div>
                </div>
                <div class="summary-card">
                    <div class="label">Different</div>
                    <div class="value warning">{}</div>
                </div>
                <div class="summary-card">
                    <div class="label">Errors</div>
                    <div class="value{}">{}</div>
                </div>
                <div class="summary-card">
                    <div class="label">Avg Similarity</div>
                    <div class="value">{:.1}%</div>
                </div>
            </div>
        </div>
"#,
        summary.pairs_compared,
        summary.identical_pairs,
        summary.different_pairs,
        if summary.error_pairs > 0 { " danger" } else { "" },
        summary.error_pairs,
        summary.average_similarity * 100.0
    )
}

/// Build results table
fn build_results_table(results: &[ComparisonResult], artifacts_dir: Option<&Path>) -> String {
    let mut html = String::new();

    html.push_str(r#"
        <div class="table-container">
            <div class="table-header">
                <h2>Comparison Results</h2>
                <input type="text" class="filter-input" id="table-filter" placeholder="Filter results...">
            </div>
            <table id="results-table">
                <thead>
                    <tr>
                        <th data-sort="status">Status</th>
                        <th data-sort="file1">File 1</th>
                        <th data-sort="file2">File 2</th>
                        <th data-sort="similarity">Similarity</th>
                        <th data-sort="type">Type</th>
                        <th>Actions</th>
                    </tr>
                </thead>
                <tbody>
"#);

    for (idx, result) in results.iter().enumerate() {
        let (file1, file2) = result.file_paths();
        let similarity = result.similarity_score();
        let identical = result.is_identical();

        let (status_badge, status_text) = if identical {
            ("identical", "Identical")
        } else {
            match result {
                ComparisonResult::Error { .. } => ("error", "Error"),
                _ => ("different", "Different"),
            }
        };

        let sim_class = if similarity >= 0.9 {
            "high"
        } else if similarity >= 0.5 {
            "medium"
        } else {
            "low"
        };

        let type_str = match result {
            ComparisonResult::Text(_) => "text",
            ComparisonResult::Structured(_) => "csv",
            ComparisonResult::HashOnly { .. } => "binary",
            ComparisonResult::Error { .. } => "error",
        };

        // Build action buttons
        let has_diff = matches!(result, ComparisonResult::Text(r) if !r.identical && !r.detailed_diff.is_empty())
            || matches!(result, ComparisonResult::Structured(r) if !r.identical);

        let view_btn = if has_diff {
            format!(r#"<button class="btn" onclick="showDiff({})">View Diff</button>"#, idx)
        } else {
            String::new()
        };

        let artifact_link = if let Some(dir) = artifacts_dir {
            let linked_id = result.linked_id();
            let sanitized = sanitize_for_filename(linked_id);
            if matches!(result, ComparisonResult::Text(_)) && !identical {
                format!(
                    r#" <a href="{}/patches/{}.diff" target="_blank">patch</a>"#,
                    dir.display(),
                    sanitized
                )
            } else if matches!(result, ComparisonResult::Structured(_)) && !identical {
                format!(
                    r#" <a href="{}/mismatches/{}.json" target="_blank">json</a>"#,
                    dir.display(),
                    sanitized
                )
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        html.push_str(&format!(
            r#"                    <tr data-idx="{}">
                        <td><span class="badge {}">{}</span></td>
                        <td class="path" title="{}">{}</td>
                        <td class="path" title="{}">{}</td>
                        <td>
                            <span class="similarity-bar"><span class="fill {}" style="width: {}%"></span></span>
                            {:.1}%
                        </td>
                        <td>{}</td>
                        <td>{}{}</td>
                    </tr>
"#,
            idx,
            status_badge,
            status_text,
            escape_html(file1),
            truncate_path(file1, 35),
            escape_html(file2),
            truncate_path(file2, 35),
            sim_class,
            (similarity * 100.0).round(),
            similarity * 100.0,
            type_str,
            view_btn,
            artifact_link
        ));
    }

    html.push_str(r#"                </tbody>
            </table>
        </div>
"#);

    html
}

/// Build diff modal HTML
fn build_diff_modal() -> String {
    r#"
        <div class="modal-overlay" id="diff-modal">
            <div class="modal">
                <div class="modal-header">
                    <h3 id="modal-title">Diff View</h3>
                    <button class="modal-close" onclick="closeDiff()">&times;</button>
                </div>
                <div class="modal-body" id="modal-body">
                </div>
            </div>
        </div>
"#.to_string()
}

/// Build embedded diff data as JSON
fn build_diff_data(results: &[ComparisonResult]) -> String {
    let mut data = Vec::new();

    for result in results {
        let entry = match result {
            ComparisonResult::Text(r) => {
                format!(
                    r#"{{"type":"text","file1":"{}","file2":"{}","diff":{}}}"#,
                    escape_json(&r.file1_path),
                    escape_json(&r.file2_path),
                    serde_json::to_string(&r.detailed_diff).unwrap_or_default()
                )
            }
            ComparisonResult::Structured(r) => {
                let mismatches_json = serde_json::to_string(&r.field_mismatches).unwrap_or_default();
                format!(
                    r#"{{"type":"structured","file1":"{}","file2":"{}","mismatches":{},"cols_only_1":{},"cols_only_2":{}}}"#,
                    escape_json(&r.file1_path),
                    escape_json(&r.file2_path),
                    mismatches_json,
                    serde_json::to_string(&r.columns_only_in_file1).unwrap_or_default(),
                    serde_json::to_string(&r.columns_only_in_file2).unwrap_or_default()
                )
            }
            _ => r#"{"type":"none"}"#.to_string(),
        };
        data.push(entry);
    }

    format!(
        r#"
    <script>
        const diffData = [{}];
    </script>
"#,
        data.join(",\n")
    )
}

/// Build JavaScript for interactivity
fn build_javascript() -> String {
    r#"
    <script>
        // Table sorting
        document.querySelectorAll('th[data-sort]').forEach(th => {
            th.addEventListener('click', () => {
                const table = th.closest('table');
                const tbody = table.querySelector('tbody');
                const rows = Array.from(tbody.querySelectorAll('tr'));
                const col = th.cellIndex;
                const isAsc = th.classList.contains('sorted-asc');
                
                table.querySelectorAll('th').forEach(h => {
                    h.classList.remove('sorted-asc', 'sorted-desc');
                });
                
                th.classList.add(isAsc ? 'sorted-desc' : 'sorted-asc');
                
                rows.sort((a, b) => {
                    let aVal = a.cells[col].textContent.trim();
                    let bVal = b.cells[col].textContent.trim();
                    
                    const aNum = parseFloat(aVal.replace('%', ''));
                    const bNum = parseFloat(bVal.replace('%', ''));
                    
                    if (!isNaN(aNum) && !isNaN(bNum)) {
                        return isAsc ? bNum - aNum : aNum - bNum;
                    }
                    
                    return isAsc ? bVal.localeCompare(aVal) : aVal.localeCompare(bVal);
                });
                
                rows.forEach(row => tbody.appendChild(row));
            });
        });
        
        // Table filtering
        document.getElementById('table-filter').addEventListener('input', (e) => {
            const filter = e.target.value.toLowerCase();
            const rows = document.querySelectorAll('#results-table tbody tr');
            
            rows.forEach(row => {
                const text = row.textContent.toLowerCase();
                row.style.display = text.includes(filter) ? '' : 'none';
            });
        });
        
        // Diff modal
        function showDiff(idx) {
            const data = diffData[idx];
            const modal = document.getElementById('diff-modal');
            const title = document.getElementById('modal-title');
            const body = document.getElementById('modal-body');
            
            if (data.type === 'text') {
                title.textContent = 'Text Diff';
                body.innerHTML = renderTextDiff(data);
            } else if (data.type === 'structured') {
                title.textContent = 'Structured Diff';
                body.innerHTML = renderStructuredDiff(data);
            } else {
                body.innerHTML = '<p>No diff available</p>';
            }
            
            modal.classList.add('active');
        }
        
        function closeDiff() {
            document.getElementById('diff-modal').classList.remove('active');
        }
        
        // Close on escape or click outside
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape') closeDiff();
        });
        
        document.getElementById('diff-modal').addEventListener('click', (e) => {
            if (e.target.id === 'diff-modal') closeDiff();
        });
        
        function renderTextDiff(data) {
            const lines = data.diff.split('\n');
            let file1Lines = [];
            let file2Lines = [];
            let lineNum1 = 0;
            let lineNum2 = 0;
            
            for (const line of lines) {
                if (line.startsWith('---') || line.startsWith('+++') || line.startsWith('@@')) {
                    continue;
                }
                
                if (line.startsWith('-')) {
                    lineNum1++;
                    file1Lines.push({ num: lineNum1, content: line.slice(1), type: 'removed' });
                    file2Lines.push({ num: '', content: '', type: 'empty' });
                } else if (line.startsWith('+')) {
                    lineNum2++;
                    file1Lines.push({ num: '', content: '', type: 'empty' });
                    file2Lines.push({ num: lineNum2, content: line.slice(1), type: 'added' });
                } else if (line.startsWith(' ')) {
                    lineNum1++;
                    lineNum2++;
                    file1Lines.push({ num: lineNum1, content: line.slice(1), type: '' });
                    file2Lines.push({ num: lineNum2, content: line.slice(1), type: '' });
                }
            }
            
            return `
                <div class="diff-container">
                    <div class="diff-panel">
                        <div class="diff-panel-header">${escapeHtml(data.file1)}</div>
                        <div class="diff-content">
                            ${file1Lines.map(l => `
                                <div class="diff-line ${l.type}">
                                    <span class="diff-line-num">${l.num}</span>
                                    <span class="diff-line-content">${escapeHtml(l.content)}</span>
                                </div>
                            `).join('')}
                        </div>
                    </div>
                    <div class="diff-panel">
                        <div class="diff-panel-header">${escapeHtml(data.file2)}</div>
                        <div class="diff-content">
                            ${file2Lines.map(l => `
                                <div class="diff-line ${l.type}">
                                    <span class="diff-line-num">${l.num}</span>
                                    <span class="diff-line-content">${escapeHtml(l.content)}</span>
                                </div>
                            `).join('')}
                        </div>
                    </div>
                </div>
            `;
        }
        
        function renderStructuredDiff(data) {
            if (!data.mismatches || data.mismatches.length === 0) {
                let html = '<p>No field mismatches found.</p>';
                
                if (data.cols_only_1 && data.cols_only_1.length > 0) {
                    html += `<p><strong>Columns only in File 1:</strong> ${data.cols_only_1.join(', ')}</p>`;
                }
                if (data.cols_only_2 && data.cols_only_2.length > 0) {
                    html += `<p><strong>Columns only in File 2:</strong> ${data.cols_only_2.join(', ')}</p>`;
                }
                
                return html;
            }
            
            let html = '<div class="struct-diff"><table><thead><tr><th>Column</th><th>Mismatches</th><th>Sample Key</th><th>File 1 Value</th><th>File 2 Value</th></tr></thead><tbody>';
            
            for (const col of data.mismatches) {
                const sample = col.sample_mismatches[0] || {};
                html += `
                    <tr>
                        <td><strong>${escapeHtml(col.column_name)}</strong></td>
                        <td>${col.mismatch_count}</td>
                        <td>${escapeHtml(sample.key || '')}</td>
                        <td class="cell-mismatch">${escapeHtml(sample.value1 || '')}</td>
                        <td class="cell-mismatch">${escapeHtml(sample.value2 || '')}</td>
                    </tr>
                `;
            }
            
            html += '</tbody></table></div>';
            return html;
        }
        
        function escapeHtml(text) {
            if (!text) return '';
            const div = document.createElement('div');
            div.textContent = text;
            return div.innerHTML;
        }
    </script>
"#.to_string()
}

/// Truncate a path string for display
fn truncate_path(path: &str, max_len: usize) -> String {
    if path.len() <= max_len {
        path.to_string()
    } else {
        format!("...{}", &path[path.len() - max_len + 3..])
    }
}

/// Sanitize a string for use as a filename
fn sanitize_for_filename(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            _ => c,
        })
        .collect()
}

/// Escape HTML special characters
fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Escape JSON string
fn escape_json(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

/// Load results from a JSONL file
pub fn load_results_from_jsonl(path: &Path) -> Result<Vec<ComparisonResult>> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read {}", path.display()))?;

    let mut results = Vec::new();
    for line in content.lines() {
        if !line.trim().is_empty() {
            let result: ComparisonResult = serde_json::from_str(line)
                .with_context(|| format!("Failed to parse JSON line: {}", line))?;
            results.push(result);
        }
    }

    Ok(results)
}
