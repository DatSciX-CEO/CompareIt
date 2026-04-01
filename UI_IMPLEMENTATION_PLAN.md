# CompareIt UI Implementation Plan

## 1. Executive Summary

This document outlines the blueprint for building a **local, privacy-first React User Interface** for the *CompareIt* engine. The goal is to evolve the tool from a pure CLI into a "Streamlit-like" interactive application that allows engineers to configure, execute, and analyze comparisons visually without leaving their desktop.

**Key Objectives:**
*   **Zero Data Egress:** All processing happens locally on the user's machine.
*   **Ease of Use:** "Point and Click" configuration replacing complex CLI flags.
*   **Professional Aesthetics:** A high-contrast, dark-themed dashboard designed for clarity and visual appeal.
*   **Full Feature Parity:** Support for all existing CLI capabilities (Simhash, Structured Data, Numeric Tolerance).

---

## 2. Technical Architecture

To satisfy the requirements of "Local Execution," "Privacy," and "React UI," we will utilize **Tauri**.

### Why Tauri?
Unlike a traditional web app or a Python Streamlit script, Tauri allows us to bundle the existing Rust logic directly with a React frontend into a **single, lightweight native executable** (Windows `.exe`, macOS `.app`, Linux).

*   **Security:** Runs essentially as a local web server that only binds to localhost, with strict CSP (Content Security Policy).
*   **Performance:** The backend remains Rust (utilizing the existing `CompareIt` high-performance engine). The frontend is a webview.
*   **File Access:** Unlike a browser, Tauri apps can (with permission) directly read/write to the file system, making selecting 500GB folders trivial.

### System Diagram

```mermaid
graph TD
    User[User] --> UI[React Frontend (Tauri Window)]

    subgraph Frontend [React + TypeScript]
        UI --> Components[UI Components (ShadCN/Mantine)]
        UI --> State[State Store (Zustand)]
        Components --> Command[Invoke Rust Command]
    end

    subgraph Backend [Rust Core]
        Command --> Bridge[Tauri Bridge]
        Bridge --> CompareLib[CompareIt Library (Existing)]
        CompareLib --> FS[Local File System]
    end
```

---

## 3. Technology Stack

### Frontend (The UI)
*   **Framework:** **React** (via Vite) - Fast, modern, industry standard.
*   **Language:** **TypeScript** - Ensures type safety matching our Rust backend.
*   **Styling:** **Tailwind CSS** - For rapid, custom "catchy" styling.
*   **Component Library:** **ShadCN UI** or **Mantine**.
    *   *Reason:* These provide professional, accessible, dark-mode-first components (Sliders, Toggles, Tables) that look "Industrial Grade" out of the box.
*   **Data Visualization:** **Recharts** or **Nivo**.
    *   *Usage:* displaying pass/fail pie charts, similarity histograms.
*   **State Management:** **TanStack Query** (React Query) + **Zustand**.

### Backend (The Engine)
*   **Runtime:** **Tauri v2** (Rust).
*   **Core Logic:** Import `compare_it` (current crate) directly as a library.

---

## 4. UI/UX Design Strategy

**Theme:** "Engineering Dark Mode"
*   **Backgrounds:** Deep slate/gunmetal (`#0f172a`), not pure black.
*   **Accents:** Neon Blue (`#3b82f6`) for actions, Emerald (`#10b981`) for matches, Rose (`#f43f5e`) for diffs.
*   **Typography:** Monospace fonts (e.g., *JetBrains Mono*) for data/paths; Sans-serif (e.g., *Inter*) for UI labels.

### Screen Layout

#### A. The "Configuration" Sidebar (Left Panel)
*   **Input Targets:** Two large "Drop Zones" for Drag & Drop file/folder selection.
*   **Mode Selector:** Buttons for `Auto`, `Text`, `Structured`.
*   **Advanced Settings (Collapsible):**
    *   *Numeric Tolerance:* Slider or Input (e.g., `0.001`).
    *   *Keys:* Input for CSV primary keys.
    *   *Ignore Patterns:* Chips/Tags input for `*.tmp`, `.git`.
*   **Action:** A large, prominent "RUN COMPARISON" button.

#### B. The "Results" Dashboard (Main Area)
*   **State 1: Idle/Welcome:**
    *   Quick start tips, recent history list.
*   **State 2: Processing:**
    *   Real-time progress bars (Indexing... Hashing... Comparing...).
    *   Terminal-like log stream at the bottom.
*   **State 3: Analysis (Post-Run):**
    *   **KPI Cards:** "Total Files", "Match Rate %", "Avg Similarity".
    *   **Interactive Table:**
        *   Sortable columns (Filename, Status, Score).
        *   Clicking a row opens a "Diff Modal".
    *   **Diff Viewer:** Split-view code editor style (like VS Code diff) for text; Grid view for CSVs.

---

## 5. Implementation Roadmap

### Phase 1: Foundation (Weeks 1-2)
1.  **Repo Restructure:** Create `ui/` directory. Initialize Tauri + React + Vite project.
2.  **Bridge Setup:** Refactor `main.rs` to expose core functions (index, compare) as public library functions.
3.  **Basic UI:** Build the Sidebar and Drag-and-Drop file selection.
4.  **Proof of Concept:** Click "Run" -> Rust counts files -> React shows count.

### Phase 2: The "Streamlit" Experience (Weeks 3-4)
1.  **Configuration Wiring:** Connect all CLI flags (tolerance, keys, exclude) to React State.
2.  **Real-time Feedback:** Implement event streaming so Rust can update the React progress bar during long tasks.
3.  **Result Parsing:** Serialize `ComparisonResult` structs to JSON for the frontend.

### Phase 3: Visualization & Polish (Weeks 5-6)
1.  **Dashboard:** Build the Summary Cards and Charts.
2.  **Diff Viewers:** Implement the Monaco Editor (VS Code editor component) for rich text diffing.
3.  **Data Grid:** Implement a virtualized table for browsing thousands of CSV rows.
4.  **Theming:** Apply the "Cyberpunk/Professional" dark theme variables.

---

## 6. Security & Privacy Assurance

*   **Sandboxing:** The App has no internet access by default. It does not send analytics.
*   **Local Only:** All file reads happen strictly within the user's selected directories.
*   **Open Source:** Users can audit the build process to verify the binary matches the source.

---

## 7. Next Steps for Approval

1.  **Review this plan.**
2.  **Approve Architecture:** Confirm Tauri is the desired vehicle.
3.  **Begin Phase 1:** Initialize the `ui` folder.
