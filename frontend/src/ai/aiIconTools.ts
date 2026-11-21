/**
 * sycnUI AI Agent Icon Integration Tools (Font Awesome Free v6)
 * JSON Schema definitions and runtime execution handlers for tool calling.
 */

import { insertIconToCanvas, InsertIconResult } from '../utils/insertIconToCanvas';

// Curated Font Awesome Free (v6) icons for instantaneous offline / fallback AI tool responses
export const CURATED_FA6_ICONS = [
  // Solid icons (fa6-solid)
  'fa6-solid:house',
  'fa6-solid:user',
  'fa6-solid:gear',
  'fa6-solid:magnifying-glass',
  'fa6-solid:star',
  'fa6-solid:heart',
  'fa6-solid:bell',
  'fa6-solid:check',
  'fa6-solid:xmark',
  'fa6-solid:plus',
  'fa6-solid:minus',
  'fa6-solid:arrow-down',
  'fa6-solid:arrow-right',
  'fa6-solid:arrow-left',
  'fa6-solid:arrow-up',
  'fa6-solid:pen',
  'fa6-solid:trash',
  'fa6-solid:envelope',
  'fa6-solid:calendar',
  'fa6-solid:cart-shopping',
  'fa6-solid:credit-card',
  'fa6-solid:lock',
  'fa6-solid:globe',
  'fa6-solid:shapes',
  'fa6-solid:paperclip',
  'fa6-solid:share',
  'fa6-solid:download',
  'fa6-solid:folder',
  'fa6-solid:camera',
  'fa6-solid:image',

  // Regular icons (fa6-regular)
  'fa6-regular:heart',
  'fa6-regular:star',
  'fa6-regular:user',
  'fa6-regular:envelope',
  'fa6-regular:calendar',
  'fa6-regular:comment',
  'fa6-regular:bookmark',
  'fa6-regular:circle-check',
  'fa6-regular:clock',
  'fa6-regular:bell',

  // Brands (fa6-brands)
  'fa6-brands:github',
  'fa6-brands:google',
  'fa6-brands:twitter',
  'fa6-brands:facebook',
  'fa6-brands:apple',
  'fa6-brands:figma',
  'fa6-brands:discord',
  'fa6-brands:linkedin',
  'fa6-brands:youtube',
];

// ==========================================
// 1. Tool JSON Schemas (Font Awesome Free v6)
// ==========================================

export const SEARCH_ICONS_TOOL_SCHEMA = {
  name: 'search_icons',
  description:
    'Search over 2,000+ Font Awesome Free (v6) vector icons across Solid (fa6-solid), Regular (fa6-regular), and Brands (fa6-brands). Returns an array of matching icon IDs (e.g., ["fa6-solid:house", "fa6-brands:github", "fa6-regular:heart"]).',
  parameters: {
    type: 'object',
    properties: {
      query: {
        type: 'string',
        description: 'The search term, keyword, or semantic concept (e.g. "house", "user", "shopping", "github", "star").',
      },
      category: {
        type: 'string',
        enum: ['all', 'solid', 'regular', 'brands'],
        description: 'Optional category filter: "all" (default), "solid" (fa6-solid), "regular" (fa6-regular), or "brands" (fa6-brands).',
      },
    },
    required: ['query'],
  },
};

export const ADD_ICON_TO_CANVAS_TOOL_SCHEMA = {
  name: 'add_icon_to_canvas',
  description:
    'Insert a Font Awesome Free (v6) vector icon onto the SycnUI canvas engine with layout context (parent container frame, relative positioning, custom color and size).',
  parameters: {
    type: 'object',
    properties: {
      icon_id: {
        type: 'string',
        description: 'The Font Awesome Free (v6) icon identifier (e.g. "fa6-solid:house", "fa6-regular:heart", "fa6-brands:github").',
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
        description: 'Hex or CSS color string for the icon. Defaults to current theme text/foreground color.',
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
 * Executes the `search_icons` tool call. Queries Iconify API for Font Awesome Free (v6).
 */
export async function handleSearchIcons(params: {
  query: string;
  category?: 'all' | 'solid' | 'regular' | 'brands';
}): Promise<string[]> {
  const query = (params.query || '').trim().toLowerCase();

  let prefixes = 'fa6-solid,fa6-regular,fa6-brands';
  if (params.category === 'solid') prefixes = 'fa6-solid';
  else if (params.category === 'regular') prefixes = 'fa6-regular';
  else if (params.category === 'brands') prefixes = 'fa6-brands';

  try {
    const res = await fetch(
      `https://api.iconify.design/search?query=${encodeURIComponent(
        query
      )}&prefixes=${prefixes}&limit=40`
    );
    if (res.ok) {
      const data = await res.json();
      if (Array.isArray(data.icons) && data.icons.length > 0) {
        return data.icons;
      }
    }
  } catch (err) {
    console.warn('[handleSearchIcons] Iconify API fetch failed, falling back to curated FA6 icons:', err);
  }

  // Curated Font Awesome Free fallback
  return CURATED_FA6_ICONS.filter((id) => {
    const matchesQuery = !query || id.toLowerCase().includes(query);
    let matchesCat = true;
    if (params.category === 'solid') matchesCat = id.startsWith('fa6-solid:');
    else if (params.category === 'regular') matchesCat = id.startsWith('fa6-regular:');
    else if (params.category === 'brands') matchesCat = id.startsWith('fa6-brands:');
    return matchesQuery && matchesCat;
  });
}

/**
 * Fetches raw SVG content for an icon ID from the Iconify SVG endpoint.
 */
export async function fetchRawSvg(iconId: string): Promise<string> {
  const [prefix, name] = iconId.includes(':') ? iconId.split(':') : ['fa6-solid', iconId];
  const url = `https://api.iconify.design/${prefix}/${name}.svg`;

  try {
    const res = await fetch(url);
    if (res.ok) {
      return await res.text();
    }
  } catch (err) {
    console.warn(`[fetchRawSvg] Failed to fetch SVG for ${iconId}:`, err);
  }

  // Fallback vector SVG path (Font Awesome style house)
  return `<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 576 512" fill="currentColor"><path d="M575.8 255.5c0 18-15 32.1-32 32.1h-32l.7 160.2c0 17-13.7 31.8-30.7 31.8l-128 .2c-17.7 0-32-14.3-32-32V320h-64v128c0 17.7-14.3 32-32 32l-128-.2c-17 0-30.7-14.8-30.7-31.8l.7-160.2H32c-18 0-32-14-32-32.1 0-9 3-17 10-24L266.4 8c7-7 15-8 22-8s15 2 21 7l256.4 224c7 7 10 15 10 24.5z"/></svg>`;
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
 * Executes the `add_icon_to_canvas` tool call with Font Awesome icons.
 */
export async function handleAddIconToCanvas(
  params: AddIconToCanvasParams
): Promise<InsertIconResult> {
  const { icon_id, parent_frame_id, layout_position, position, color, size } = params;

  // 1. Fetch raw SVG from Iconify
  const rawSvg = await fetchRawSvg(icon_id);

  // 2. Insert into canvas engine
  const result = await insertIconToCanvas(rawSvg, position?.x, position?.y, {
    size: size ?? 24,
    color,
    parentFrameId: parent_frame_id,
    layoutPosition: layout_position ?? 'center',
    name: `Icon / ${icon_id}`,
  });

  return result;
}

/**
 * Unified AI agent tool dispatcher
 */
export async function executeAiIconTool(toolName: string, args: Record<string, any>): Promise<any> {
  switch (toolName) {
    case 'search_icons':
      return await handleSearchIcons(args as { query: string; category?: any });
    case 'add_icon_to_canvas':
      return await handleAddIconToCanvas(args as AddIconToCanvasParams);
    default:
      throw new Error(`Unknown AI icon tool: ${toolName}`);
  }
}
