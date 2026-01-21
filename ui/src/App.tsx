import { useState, useEffect, Component, ErrorInfo, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";

// Error Boundary to catch rendering errors and prevent white screen crashes
interface ErrorBoundaryProps {
  children: ReactNode;
  fallback?: ReactNode;
}

interface ErrorBoundaryState {
  hasError: boolean;
  error: Error | null;
}

class ErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  constructor(props: ErrorBoundaryProps) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("ErrorBoundary caught an error:", error, errorInfo);
  }

  render() {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }
      return (
        <div className="p-4 bg-rose-900/20 border border-rose-800 rounded-lg m-4">
          <h3 className="text-rose-400 font-semibold mb-2">Something went wrong</h3>
          <p className="text-rose-300 text-sm mb-4">
            An error occurred while rendering this section. 
          </p>
          <details className="text-xs text-slate-400">
            <summary className="cursor-pointer hover:text-slate-300">Error details</summary>
            <pre className="mt-2 p-2 bg-slate-900 rounded overflow-auto">
              {this.state.error?.message || "Unknown error"}
            </pre>
          </details>
          <button
            onClick={() => this.setState({ hasError: false, error: null })}
            className="mt-4 px-4 py-2 bg-slate-700 hover:bg-slate-600 rounded text-sm text-slate-200"
          >
            Try Again
          </button>
        </div>
      );
    }

    return this.props.children;
  }
}

// Types matching Rust structs
interface ProgressEvent {
  stage: string;
  message: string;
  current: number;
  total: number;
  percentage: number;
}

interface ComparisonSummary {
  totalFilesSet1: number;
  totalFilesSet2: number;
  pairsCompared: number;
  identicalPairs: number;
  differentPairs: number;
  errorPairs: number;
  averageSimilarity: number;
  minSimilarity: number;
  maxSimilarity: number;
}

interface TextResult {
  type: "Text";
  linked_id: string;
  file1_path: string;
  file2_path: string;
  similarity_score: number;
  identical: boolean;
  file1_line_count: number;
  file2_line_count: number;
  common_lines: number;
  only_in_file1: number;
  only_in_file2: number;
  detailed_diff: string;
}

interface StructuredResult {
  type: "Structured";
  linked_id: string;
  file1_path: string;
  file2_path: string;
  similarity_score: number;
  identical: boolean;
  file1_row_count: number;
  file2_row_count: number;
  common_records: number;
  only_in_file1: number;
  only_in_file2: number;
  field_mismatches: Array<{
    column_name: string;
    mismatch_count: number;
    sample_mismatches: Array<{
      key: string;
      value1: string;
      value2: string;
    }>;
  }>;
}

interface HashOnlyResult {
  type: "HashOnly";
  linked_id: string;
  file1_path: string;
  file2_path: string;
  identical: boolean;
  file1_size: number;
  file2_size: number;
}

interface ErrorResult {
  type: "Error";
  file1_path: string;
  file2_path: string;
  error: string;
}

type ComparisonResult = TextResult | StructuredResult | HashOnlyResult | ErrorResult;

interface CompareResponse {
  success: boolean;
  summary: ComparisonSummary | null;
  results: ComparisonResult[];
  error: string | null;
  resultsDir: string | null;
}

interface CompareConfig {
  path1: string;
  path2: string;
  mode?: string;
  pairing?: string;
  topK?: number;
  keyColumns?: string[];
  numericTolerance?: number;
  ignoreEol?: boolean;
  ignoreTrailingWs?: boolean;
  ignoreAllWs?: boolean;
  ignoreCase?: boolean;
  skipEmptyLines?: boolean;
  excludePatterns?: string[];
  ignoreColumns?: string[];
  ignoreRegex?: string;
  resultsBase?: string;
}

// Icon components (inline SVG for local-only)
const FolderIcon = () => (
  <svg className="w-8 h-8" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={1.5} 
      d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z" />
  </svg>
);

const PlayIcon = () => (
  <svg className="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
      d="M14.752 11.168l-3.197-2.132A1 1 0 0010 9.87v4.263a1 1 0 001.555.832l3.197-2.132a1 1 0 000-1.664z" />
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} 
      d="M21 12a9 9 0 11-18 0 9 9 0 0118 0z" />
  </svg>
);

const ChevronIcon = ({ open }: { open: boolean }) => (
  <svg className={`w-4 h-4 transition-transform ${open ? 'rotate-180' : ''}`} 
    fill="none" stroke="currentColor" viewBox="0 0 24 24">
    <path strokeLinecap="round" strokeLinejoin="round" strokeWidth={2} d="M19 9l-7 7-7-7" />
  </svg>
);

function App() {
  // Path state
  const [path1, setPath1] = useState<string>("");
  const [path2, setPath2] = useState<string>("");
  
  // Settings state
  const [showSettings, setShowSettings] = useState(false);
  const [mode, setMode] = useState<string>("auto");
  const [pairing, setPairing] = useState<string>("all-vs-all");
  const [numericTolerance, setNumericTolerance] = useState<string>("0.0001");
  const [keyColumns, setKeyColumns] = useState<string>("");
  const [excludePatterns, setExcludePatterns] = useState<string>("");
  const [ignoreAllWs, setIgnoreAllWs] = useState(false);
  const [ignoreCase, setIgnoreCase] = useState(false);
  
  // Comparison state
  const [isRunning, setIsRunning] = useState(false);
  const [progress, setProgress] = useState<ProgressEvent | null>(null);
  const [response, setResponse] = useState<CompareResponse | null>(null);
  const [selectedResult, setSelectedResult] = useState<ComparisonResult | null>(null);
  
  // Listen for progress events
  useEffect(() => {
    const unlisten = listen<ProgressEvent>("compare-progress", (event) => {
      setProgress(event.payload);
    });
    
    return () => {
      unlisten.then(fn => fn());
    };
  }, []);

  // Select folder dialog
  const selectPath = async (setter: (path: string) => void) => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Select folder to compare",
    });
    
    if (selected && typeof selected === "string") {
      setter(selected);
    }
  };

  // Run comparison
  const runComparison = async () => {
    if (!path1 || !path2) return;
    
    setIsRunning(true);
    setProgress(null);
    setResponse(null);
    setSelectedResult(null);
    
    const config: CompareConfig = {
      path1,
      path2,
      mode: mode !== "auto" ? mode : undefined,
      pairing,
      numericTolerance: parseFloat(numericTolerance) || 0.0001,
      keyColumns: keyColumns ? keyColumns.split(",").map(s => s.trim()) : undefined,
      excludePatterns: excludePatterns ? excludePatterns.split(",").map(s => s.trim()) : undefined,
      ignoreAllWs,
      ignoreCase,
    };
    
    try {
      const result = await invoke<CompareResponse>("run_comparison", { config });
      setResponse(result);
    } catch (error) {
      setResponse({
        success: false,
        summary: null,
        results: [],
        error: String(error),
        resultsDir: null,
      });
    } finally {
      setIsRunning(false);
    }
  };

  // Get truncated path for display
  const truncatePath = (path: string, maxLen: number = 40) => {
    if (path.length <= maxLen) return path;
    return "..." + path.slice(-maxLen + 3);
  };

  // Get status badge
  const getStatusBadge = (result: ComparisonResult) => {
    if (result.type === "Error") {
      return <span className="badge-error">Error</span>;
    }
    if (result.identical) {
      return <span className="badge-identical">Identical</span>;
    }
    return <span className="badge-different">Different</span>;
  };

  // Get similarity score
  const getSimilarity = (result: ComparisonResult): number => {
    if (result.type === "Error") return 0;
    if (result.type === "HashOnly") return result.identical ? 1 : 0;
    return result.similarity_score;
  };

  // Get similarity bar color class
  const getSimilarityClass = (score: number): string => {
    if (score >= 0.9) return "similarity-high";
    if (score >= 0.5) return "similarity-medium";
    return "similarity-low";
  };

  return (
    <div className="min-h-screen bg-slate-950 flex flex-col">
      {/* Header */}
      <header className="bg-slate-900 border-b border-slate-800 px-6 py-4">
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-3">
            <div className="w-10 h-10 bg-gradient-to-br from-cyan-500 to-cyan-700 rounded-lg flex items-center justify-center">
              <span className="text-white font-bold text-lg">C</span>
            </div>
            <div>
              <h1 className="text-xl font-bold text-white">CompareIt</h1>
              <p className="text-xs text-slate-400">Local File Comparison Engine</p>
            </div>
          </div>
          <div className="text-xs text-slate-500">
            100% Local • Zero Network
          </div>
        </div>
      </header>

      <main className="flex-1 flex">
        {/* Sidebar - Configuration */}
        <aside className="w-96 bg-slate-900/50 border-r border-slate-800 p-6 flex flex-col">
          <div className="space-y-6 flex-1">
            {/* Path Selection */}
            <div>
              <h2 className="text-sm font-semibold text-slate-300 mb-4">Select Paths</h2>
              
              {/* Path 1 */}
              <div className="mb-4">
                <label className="text-xs text-slate-400 mb-2 block">Source (Path A)</label>
                <button
                  onClick={() => selectPath(setPath1)}
                  className={`w-full h-24 border-2 border-dashed rounded-xl flex flex-col items-center justify-center gap-2 transition-all ${
                    path1 
                      ? "border-cyan-600 bg-cyan-900/20" 
                      : "border-slate-700 hover:border-slate-600 hover:bg-slate-800/30"
                  }`}
                >
                  <FolderIcon />
                  <span className="text-sm text-slate-300">
                    {path1 ? truncatePath(path1, 35) : "Click to select folder"}
                  </span>
                </button>
              </div>
              
              {/* Path 2 */}
              <div>
                <label className="text-xs text-slate-400 mb-2 block">Target (Path B)</label>
                <button
                  onClick={() => selectPath(setPath2)}
                  className={`w-full h-24 border-2 border-dashed rounded-xl flex flex-col items-center justify-center gap-2 transition-all ${
                    path2 
                      ? "border-emerald-600 bg-emerald-900/20" 
                      : "border-slate-700 hover:border-slate-600 hover:bg-slate-800/30"
                  }`}
                >
                  <FolderIcon />
                  <span className="text-sm text-slate-300">
                    {path2 ? truncatePath(path2, 35) : "Click to select folder"}
                  </span>
                </button>
              </div>
            </div>

            {/* Advanced Settings Accordion */}
            <div className="border border-slate-700 rounded-lg overflow-hidden">
              <button
                onClick={() => setShowSettings(!showSettings)}
                className="w-full px-4 py-3 flex items-center justify-between bg-slate-800/50 hover:bg-slate-800 transition-colors"
              >
                <span className="text-sm font-medium text-slate-300">Engine Settings</span>
                <ChevronIcon open={showSettings} />
              </button>
              
              {showSettings && (
                <div className="p-4 space-y-4 bg-slate-800/20">
                  {/* Mode */}
                  <div>
                    <label className="text-xs text-slate-400 mb-1.5 block">Comparison Mode</label>
                    <select
                      value={mode}
                      onChange={(e) => setMode(e.target.value)}
                      className="w-full bg-slate-800 border border-slate-600 rounded px-3 py-2 text-sm text-slate-200"
                    >
                      <option value="auto">Auto-detect</option>
                      <option value="text">Text (Line-by-line)</option>
                      <option value="structured">Structured (CSV/TSV)</option>
                    </select>
                  </div>
                  
                  {/* Pairing Strategy */}
                  <div>
                    <label className="text-xs text-slate-400 mb-1.5 block">Pairing Strategy</label>
                    <select
                      value={pairing}
                      onChange={(e) => setPairing(e.target.value)}
                      className="w-full bg-slate-800 border border-slate-600 rounded px-3 py-2 text-sm text-slate-200"
                    >
                      <option value="all-vs-all">All-vs-All (Smart Match)</option>
                      <option value="same-path">Same Path</option>
                      <option value="same-name">Same Name</option>
                    </select>
                  </div>
                  
                  {/* Numeric Tolerance */}
                  <div>
                    <label className="text-xs text-slate-400 mb-1.5 block">Numeric Tolerance</label>
                    <input
                      type="number"
                      value={numericTolerance}
                      onChange={(e) => setNumericTolerance(e.target.value)}
                      min="0"
                      max="1"
                      step="0.0001"
                      className="w-full bg-slate-800 border border-slate-600 rounded px-3 py-2 text-sm text-slate-200 font-mono"
                      placeholder="0.0001"
                    />
                  </div>
                  
                  {/* Key Columns */}
                  <div>
                    <label className="text-xs text-slate-400 mb-1.5 block">Key Columns (CSV)</label>
                    <input
                      type="text"
                      value={keyColumns}
                      onChange={(e) => setKeyColumns(e.target.value)}
                      maxLength={500}
                      className="w-full bg-slate-800 border border-slate-600 rounded px-3 py-2 text-sm text-slate-200"
                      placeholder="id, email"
                    />
                  </div>
                  
                  {/* Exclude Patterns */}
                  <div>
                    <label className="text-xs text-slate-400 mb-1.5 block">Exclude Patterns</label>
                    <input
                      type="text"
                      value={excludePatterns}
                      onChange={(e) => setExcludePatterns(e.target.value)}
                      maxLength={1000}
                      className="w-full bg-slate-800 border border-slate-600 rounded px-3 py-2 text-sm text-slate-200"
                      placeholder="*.tmp, node_modules"
                    />
                  </div>
                  
                  {/* Toggles */}
                  <div className="flex gap-4">
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={ignoreAllWs}
                        onChange={(e) => setIgnoreAllWs(e.target.checked)}
                        className="w-4 h-4 rounded bg-slate-700 border-slate-600"
                      />
                      <span className="text-xs text-slate-400">Ignore Whitespace</span>
                    </label>
                    <label className="flex items-center gap-2 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={ignoreCase}
                        onChange={(e) => setIgnoreCase(e.target.checked)}
                        className="w-4 h-4 rounded bg-slate-700 border-slate-600"
                      />
                      <span className="text-xs text-slate-400">Ignore Case</span>
                    </label>
                  </div>
                </div>
              )}
            </div>
          </div>

          {/* Run Button */}
          <button
            onClick={runComparison}
            disabled={!path1 || !path2 || isRunning}
            className="btn-primary w-full flex items-center justify-center gap-2 mt-6"
          >
            {isRunning ? (
              <>
                <div className="w-5 h-5 border-2 border-white/30 border-t-white rounded-full animate-spin" />
                <span>Comparing...</span>
              </>
            ) : (
              <>
                <PlayIcon />
                <span>Run Comparison</span>
              </>
            )}
          </button>

          {/* Progress */}
          {isRunning && progress && (
            <div className="mt-4 p-3 bg-slate-800/50 rounded-lg">
              <div className="flex justify-between text-xs text-slate-400 mb-2">
                <span>{progress.stage}</span>
                <span>{progress.percentage.toFixed(0)}%</span>
              </div>
              <div className="h-2 bg-slate-700 rounded-full overflow-hidden">
                <div 
                  className="h-full bg-cyan-500 transition-all duration-200 progress-shimmer"
                  style={{ width: `${progress.percentage}%` }}
                />
              </div>
            </div>
          )}
        </aside>

        {/* Main Content Area */}
        <div className="flex-1 p-6 overflow-auto">
          {/* Welcome State */}
          {!response && !isRunning && (
            <div className="h-full flex items-center justify-center">
              <div className="text-center max-w-md">
                <div className="w-20 h-20 bg-slate-800 rounded-2xl mx-auto mb-6 flex items-center justify-center">
                  <FolderIcon />
                </div>
                <h2 className="text-xl font-semibold text-slate-200 mb-2">Select folders to compare</h2>
                <p className="text-slate-400 text-sm">
                  Choose two folders or files from the sidebar, then click "Run Comparison" to analyze differences.
                </p>
              </div>
            </div>
          )}

          {/* Error State */}
          {response && !response.success && (
            <div className="p-4 bg-rose-900/20 border border-rose-800 rounded-lg">
              <h3 className="text-rose-400 font-semibold mb-2">Comparison Failed</h3>
              <p className="text-rose-300 text-sm">{response.error}</p>
            </div>
          )}

          {/* Results */}
          {response && response.success && response.summary && (
            <ErrorBoundary>
            <div className="space-y-6">
              {/* Summary Cards */}
              <div className="grid grid-cols-5 gap-4">
                <div className="card">
                  <div className="card-header">Pairs Compared</div>
                  <div className="text-2xl font-bold text-white">{response.summary.pairsCompared}</div>
                </div>
                <div className="card">
                  <div className="card-header">Identical</div>
                  <div className="text-2xl font-bold text-emerald-400">{response.summary.identicalPairs}</div>
                </div>
                <div className="card">
                  <div className="card-header">Different</div>
                  <div className="text-2xl font-bold text-amber-400">{response.summary.differentPairs}</div>
                </div>
                <div className="card">
                  <div className="card-header">Errors</div>
                  <div className={`text-2xl font-bold ${response.summary.errorPairs > 0 ? 'text-rose-400' : 'text-slate-400'}`}>
                    {response.summary.errorPairs}
                  </div>
                </div>
                <div className="card">
                  <div className="card-header">Avg. Similarity</div>
                  <div className="text-2xl font-bold text-cyan-400">
                    {(response.summary.averageSimilarity * 100).toFixed(1)}%
                  </div>
                </div>
              </div>

              {/* Results Dir */}
              {response.resultsDir && (
                <div className="text-xs text-slate-500">
                  Results saved to: <span className="text-slate-400 font-mono">{response.resultsDir}</span>
                </div>
              )}

              {/* Results Table */}
              <div className="card p-0">
                <div className="px-4 py-3 border-b border-slate-700">
                  <h3 className="font-semibold text-slate-200">Comparison Results</h3>
                </div>
                <div className="overflow-auto max-h-[500px]">
                  <table className="w-full text-sm">
                    <thead className="bg-slate-800/50 sticky top-0">
                      <tr>
                        <th className="text-left px-4 py-2 text-slate-400 font-medium">Status</th>
                        <th className="text-left px-4 py-2 text-slate-400 font-medium">File 1</th>
                        <th className="text-left px-4 py-2 text-slate-400 font-medium">File 2</th>
                        <th className="text-left px-4 py-2 text-slate-400 font-medium">Similarity</th>
                        <th className="text-left px-4 py-2 text-slate-400 font-medium">Type</th>
                      </tr>
                    </thead>
                    <tbody>
                      {response.results.map((result, idx) => (
                        <tr 
                          key={idx}
                          onClick={() => setSelectedResult(result)}
                          className={`border-b border-slate-800 hover:bg-slate-800/30 cursor-pointer transition-colors ${
                            selectedResult === result ? 'bg-slate-800/50' : ''
                          }`}
                        >
                          <td className="px-4 py-2">{getStatusBadge(result)}</td>
                          <td className="px-4 py-2 text-slate-300 font-mono text-xs">
                            {truncatePath(result.file1_path, 30)}
                          </td>
                          <td className="px-4 py-2 text-slate-300 font-mono text-xs">
                            {truncatePath(result.file2_path, 30)}
                          </td>
                          <td className="px-4 py-2">
                            <div className="flex items-center gap-2">
                              <div className="similarity-bar w-16">
                                <div 
                                  className={`similarity-fill ${getSimilarityClass(getSimilarity(result))}`}
                                  style={{ width: `${getSimilarity(result) * 100}%` }}
                                />
                              </div>
                              <span className="text-slate-400 text-xs">
                                {(getSimilarity(result) * 100).toFixed(1)}%
                              </span>
                            </div>
                          </td>
                          <td className="px-4 py-2 text-slate-400 text-xs">
                            {result.type === "Text" ? "text" : 
                             result.type === "Structured" ? "csv" : 
                             result.type === "HashOnly" ? "binary" : "error"}
                          </td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              </div>

              {/* Detail View */}
              {selectedResult && selectedResult.type !== "Error" && (
                <div className="card">
                  <div className="flex justify-between items-start mb-4">
                    <div>
                      <h3 className="font-semibold text-slate-200 mb-1">Detail View</h3>
                      <p className="text-xs text-slate-400 font-mono">
                        {selectedResult.file1_path}
                      </p>
                    </div>
                    <button
                      onClick={() => setSelectedResult(null)}
                      className="text-slate-400 hover:text-slate-200"
                    >
                      ✕
                    </button>
                  </div>
                  
                  {selectedResult.type === "Text" && (
                    <div className="space-y-4">
                      <div className="grid grid-cols-3 gap-4 text-sm">
                        <div>
                          <span className="text-slate-400">Lines in File 1:</span>
                          <span className="ml-2 text-white">{selectedResult.file1_line_count}</span>
                        </div>
                        <div>
                          <span className="text-slate-400">Lines in File 2:</span>
                          <span className="ml-2 text-white">{selectedResult.file2_line_count}</span>
                        </div>
                        <div>
                          <span className="text-slate-400">Common Lines:</span>
                          <span className="ml-2 text-emerald-400">{selectedResult.common_lines}</span>
                        </div>
                      </div>
                      
                      {selectedResult.detailed_diff && (
                        <div>
                          <h4 className="text-sm font-medium text-slate-300 mb-2">Diff Preview</h4>
                          <pre className="bg-slate-900 rounded p-4 text-xs font-mono overflow-auto max-h-64">
                            {selectedResult.detailed_diff.split('\n').slice(0, 50).map((line, i) => (
                              <div 
                                key={i}
                                className={
                                  line.startsWith('+') && !line.startsWith('+++') ? 'text-emerald-400' :
                                  line.startsWith('-') && !line.startsWith('---') ? 'text-rose-400' :
                                  line.startsWith('@@') ? 'text-cyan-400' :
                                  'text-slate-400'
                                }
                              >
                                {line}
                              </div>
                            ))}
                          </pre>
                        </div>
                      )}
                    </div>
                  )}
                  
                  {selectedResult.type === "Structured" && (
                    <div className="space-y-4">
                      <div className="grid grid-cols-3 gap-4 text-sm">
                        <div>
                          <span className="text-slate-400">Rows in File 1:</span>
                          <span className="ml-2 text-white">{selectedResult.file1_row_count}</span>
                        </div>
                        <div>
                          <span className="text-slate-400">Rows in File 2:</span>
                          <span className="ml-2 text-white">{selectedResult.file2_row_count}</span>
                        </div>
                        <div>
                          <span className="text-slate-400">Common Records:</span>
                          <span className="ml-2 text-emerald-400">{selectedResult.common_records}</span>
                        </div>
                      </div>
                      
                      {selectedResult.field_mismatches.length > 0 && (
                        <div>
                          <h4 className="text-sm font-medium text-slate-300 mb-2">Field Mismatches</h4>
                          <table className="w-full text-sm">
                            <thead className="bg-slate-800/50">
                              <tr>
                                <th className="text-left px-3 py-2 text-slate-400">Column</th>
                                <th className="text-left px-3 py-2 text-slate-400">Mismatches</th>
                                <th className="text-left px-3 py-2 text-slate-400">Sample Key</th>
                                <th className="text-left px-3 py-2 text-slate-400">Value 1</th>
                                <th className="text-left px-3 py-2 text-slate-400">Value 2</th>
                              </tr>
                            </thead>
                            <tbody>
                              {selectedResult.field_mismatches.slice(0, 10).map((fm, i) => (
                                <tr key={i} className="border-b border-slate-800">
                                  <td className="px-3 py-2 font-medium text-slate-200">{fm.column_name}</td>
                                  <td className="px-3 py-2 text-amber-400">{fm.mismatch_count}</td>
                                  <td className="px-3 py-2 text-slate-400 font-mono text-xs">
                                    {fm.sample_mismatches[0]?.key || '-'}
                                  </td>
                                  <td className="px-3 py-2 text-rose-400 font-mono text-xs">
                                    {fm.sample_mismatches[0]?.value1 || '-'}
                                  </td>
                                  <td className="px-3 py-2 text-emerald-400 font-mono text-xs">
                                    {fm.sample_mismatches[0]?.value2 || '-'}
                                  </td>
                                </tr>
                              ))}
                            </tbody>
                          </table>
                        </div>
                      )}
                    </div>
                  )}
                </div>
              )}
            </div>
            </ErrorBoundary>
          )}
        </div>
      </main>
    </div>
  );
}

export default App;
