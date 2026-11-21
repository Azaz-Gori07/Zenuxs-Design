# sycnUI Frontend & Icons Integration

This package contains the sycnUI Web frontend components, virtualization engine, canvas insertion pipeline, and AI Agent tools.

## Architecture

- **`src/components/Sidebar.tsx`**: Figma-like primary leftmost rail with `Home`, `File`, `Assets`, `Variables`, and `Icons` (using `lucide-react` `Shapes` icon positioned directly below `Variables`). Clicking tabs toggles the secondary panel.
- **`src/components/IconsPanel.tsx`**: High-performance 5-column virtualized icon grid utilizing `@tanstack/react-virtual` for smooth rendering across 200,000+ Iconify / Lucide icons with search, category filtering, infinite scroll, and WCAG AA accessibility.
- **`src/utils/insertIconToCanvas.ts`**: Canvas insertion utility with UTF-8 safe Base64 encoding `btoa(unescape(encodeURIComponent(svg)))`, dynamic canvas theme text color resolution, node ID returns, and event dispatch.
- **`src/ai/aiIconTools.ts`**: AI Agent JSON Schemas (`search_icons`, `add_icon_to_canvas`) and execution handlers with parent frame nesting and contextual layout positioning (`center`, `top-left`, `top-right`, `bottom-left`, `bottom-right`, `after-text`).
