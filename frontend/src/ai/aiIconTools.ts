/**
 * sycnUI AI Agent Icon Integration Tools
 * JSON Schema definitions and runtime execution handlers for tool calling.
 */

import { insertIconToCanvas, InsertIconResult } from '../utils/insertIconToCanvas';

// Curated offline fallbacks for instantaneous AI tool responses
export const CURATED_ICON_CATALOG = [
  'lucide:shapes',
  'lucide:star',
  'lucide:heart',
  'lucide:sparkles',
  'lucide:home',
  'lucide:file',
  'lucide:layers',
  'lucide:settings',
  'lucide:search',
  'lucide:user',
  'lucide:send',
  'lucide:paperclip',
  'lucide:check',
  'lucide:x',
  'lucide:plus',
  'lucide:minus',
  'lucide:timer',
  'lucide:pause',
  'lucide:globe',
  'lucide:lock',
  'lucide:layout-grid',
  'lucide:list',
  'lucide:arrow-down',
  'lucide:arrow-right',
  'lucide:arrow-left',
  'lucide:arrow-up',
  'lucide:pen',
  'lucide:pen-tool',
  'lucide:highlighter',
  'lucide:git-commit',
  'lucide:stamp',
  'lucide:bell',
  'lucide:shopping-cart',
  'lucide:credit-card',
  'lucide:folder',
  'ph:user',
  'ph:house',
  'ph:gear',
  'ph:magnifying-glass',
  'ph:heart',
  'material:home',
  'material:search',
  'material:favorite',
  'material:person',
];

// ==========================================
// 1. Tool JSON Schemas
// ==========================================

export const SEARCH_ICONS_TOOL_SCHEMA = {
  name: 'search_icons',
  description:
    'Search over 200,000+ vector icons across libraries (Lucide, Material, Phosphor, Tabler). Returns an array of matching icon IDs (e.g., ["lucide:home", "ph:user"]).',
  parameters: {
    type: 'object',
    properties: {
      query: {
        type: 'string',
        description: 'The search term, icon name, category, or semantic intent (e.g. "shopping cart", "settings", "profile").',
      },
    },
    required: ['query'],
  },
};

export const ADD_ICON_TO_CANVAS_TOOL_SCHEMA = {
  name: 'add_icon_to_canvas',
  description:
    'Insert a vector icon onto the SycnUI canvas engine with layout context (parent container frame, relative positioning, custom color and size).',
  parameters: {
    type: 'object',
    properties: {
      icon_id: {
        type: 'string',
        description: 'The Iconify or Lucide icon identifier (e.g. "lucide:home", "lucide:settings", "ph:user").',
      },
      parent_frame_id: {
        type: ['string', 'number'],
        description: 'Optional ID of the parent frame or container element where the icon should be grouped/nested.',
      },
      layout_position: {
        type: 'string',
        enum: ['center', 'top-left', 'top-right', 'bottom-left', 'bottom-right', 'after-text'],
        description: 'Contextual placement relative to the parent frame or adjacent text elements.',
      },
      position: {
        type: 'object',
        properties: {
          x: { type: 'number', description: 'Explicit X coordinate' },
          y: { type: 'number', description: 'Explicit Y coordinate' },
        },
        description: 'Optional explicit coordinates if exact placement is known.',
      },
      color: {
        type: 'string',
        description: 'Hex or CSS color string for the icon. Defaults to theme text color.',
      },
      size: {
        type: 'number',
        description: 'Icon dimensions in pixels (width and height). Default: 24.',
      },
    },
    required: ['icon_id'],
  },
};

// ==========================================
// 2. Runtime Handlers
// ==========================================

/**
 * Executes the `search_icons` tool call. Queries Iconify API with local offline fallback.
 */
export async function handleSearchIcons(params: { query: string }): Promise<string[]> {
  const query = (params.query || '').trim().toLowerCase();

  try {
    const res = await fetch(`https://api.iconify.design/search?query=${encodeURIComponent(query)}&limit=40`);
    if (res.ok) {
      const data = await res.json();
      if (Array.isArray(data.icons) && data.icons.length > 0) {
        return data.icons;
      }
    }
  } catch (err) {
    console.warn('[handleSearchIcons] Iconify API fetch failed, falling back to curated list:', err);
  }

  // Curated fallback
  return CURATED_ICON_CATALOG.filter(
    (id) => !query || id.toLowerCase().includes(query)
  );
}

/**
 * Fetches raw SVG content for an icon ID from the Iconify SVG endpoint.
 */
export async function fetchRawSvg(iconId: string): Promise<string> {
  const [prefix, name] = iconId.includes(':') ? iconId.split(':') : ['lucide', iconId];
  const url = `https://api.iconify.design/${prefix}/${name}.svg`;

  try {
    const res = await fetch(url);
    if (res.ok) {
      return await res.text();
    }
  } catch (err) {
    console.warn(`[fetchRawSvg] Failed to fetch SVG for ${iconId}:`, err);
  }

  // Fallback vector SVG path (a clean default shape)
  return `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m9 12 2 2 4-4"/></svg>`;
}

export interface AddIconToCanvasParams {
  icon_id: string;
  parent_frame_id?: string | number;
  layout_position?: 'center' | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right' | 'after-text';
  position?: { x: number; y: number };
  color?: string;
  size?: number;
}

/**
 * Executes the `add_icon_to_canvas` tool call.
 */
export async function handleAddIconToCanvas(
  params: AddIconToCanvasParams
): Promise<InsertIconResult> {
  const { icon_id, parent_frame_id, layout_position, position, color, size } = params;

  // 1. Fetch raw SVG
  const rawSvg = await fetchRawSvg(icon_id);

  // 2. Insert into canvas engine
  const result = await insertIconToCanvas(
    rawSvg,
    position?.x,
    position?.y,
    {
      size: size ?? 24,
      color,
      parentFrameId: parent_frame_id,
      layoutPosition: layout_position ?? 'center',
      name: `Icon / ${icon_id}`,
    }
  );

  return result;
}

/**
 * Unified AI agent tool dispatcher
 */
export async function executeAiIconTool(toolName: string, args: Record<string, any>): Promise<any> {
  switch (toolName) {
    case 'search_icons':
      return await handleSearchIcons(args as { query: string });
    case 'add_icon_to_canvas':
      return await handleAddIconToCanvas(args as AddIconToCanvasParams);
    default:
      throw new Error(`Unknown AI icon tool: ${toolName}`);
  }
}
