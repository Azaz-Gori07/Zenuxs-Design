/**
 * sycnUI AI Agent Market Tools
 * 
 * Provides JSON Schema definitions, runtime execution handlers, and system instructions
 * ensuring that the AI prioritizes prebuilt components from the Market Registry first.
 */

import { marketRegistry, MarketCategory, MarketRegistrySource, MarketComponent } from '../lib/market-registry';
import { insertMarketComponentToCanvas, InsertMarketComponentResult } from '../utils/insertMarketComponentToCanvas';

// =========================================================================
// 1. Tool JSON Schemas for Tool Calling (OpenAI / Anthropic / Gemini / MCP)
// =========================================================================

export const SEARCH_MARKET_COMPONENTS_TOOL_SCHEMA = {
  name: 'search_market_components',
  description:
    'Search the 100% Free Shadcn Community Marketplace Registry (@shadcn, @originui, @kibo, @cult, @animate-ui, @bundui, @neobrutalism) for verified prebuilt components (Buttons, Inputs, Cards, Charts, Kanban/Complex, Animations). FIRST PRIORITY: Always search here before building custom UI from scratch.',
  parameters: {
    type: 'object',
    properties: {
      query: {
        type: 'string',
        description:
          'Search term or design concept (e.g., "button", "pricing card", "revenue chart", "kanban board", "login input", "search bar", "spotlight").',
      },
      category: {
        type: 'string',
        enum: ['All', 'Buttons', 'Inputs', 'Cards', 'Charts', 'Kanban/Complex', 'Animations'],
        description: 'Optional category filter.',
      },
      registry: {
        type: 'string',
        enum: ['all', '@shadcn', '@originui', '@kibo', '@cult', '@animate-ui', '@bundui', '@neobrutalism'],
        description: 'Optional registry source filter.',
      },
    },
    required: ['query'],
  },
};

export const ADD_MARKET_COMPONENT_TO_CANVAS_TOOL_SCHEMA = {
  name: 'add_market_component_to_canvas',
  description:
    'Instantiate and place a prebuilt component from the Market Registry directly onto the active canvas. Adapts the component with custom text, titles, values, and brand color palette.',
  parameters: {
    type: 'object',
    properties: {
      component_id: {
        type: 'string',
        description:
          'The ID of the Market component (e.g. "@shadcn/button", "@originui/pricing-card", "@kibo/kanban-board", "@shadcn/area-chart", "@animate-ui/spotlight-card").',
      },
      position: {
        type: 'object',
        properties: {
          x: { type: 'number', description: 'X coordinate on canvas' },
          y: { type: 'number', description: 'Y coordinate on canvas' },
        },
        description: 'Optional canvas coordinates. Defaults to center of viewport.',
      },
      parent_frame_id: {
        type: ['string', 'number'],
        description: 'Optional ID of the parent container frame to nest this component in.',
      },
      custom_text: {
        type: 'object',
        description:
          'Optional key-value pairs replacing default text labels (e.g. {"Title": "Q3 Sales Analytics", "Price": "$49 / month"}).',
      },
      accent_color: {
        type: 'string',
        description: 'Optional accent color hex or CSS string for buttons, highlights, or badges.',
      },
    },
    required: ['component_id'],
  },
};

// =========================================================================
// 2. Runtime Execution Handlers
// =========================================================================

export interface SearchMarketParams {
  query: string;
  category?: MarketCategory;
  registry?: MarketRegistrySource | 'all';
}

export interface AddMarketComponentParams {
  component_id: string;
  position?: { x: number; y: number };
  parent_frame_id?: string | number;
  custom_text?: Record<string, string>;
  accent_color?: string;
}

/**
 * Searches the Market Registry and returns normalized summaries for the AI model.
 */
export async function handleSearchMarketComponents(params: SearchMarketParams): Promise<any[]> {
  const matches = marketRegistry.search({
    query: params.query,
    category: params.category,
    registry: params.registry,
  });

  return matches.map((c) => ({
    id: c.id,
    name: c.name,
    sourceRegistry: c.sourceRegistry,
    category: c.category,
    description: c.description,
    tags: c.tags,
    dependencies: c.dependencies,
  }));
}

/**
 * Places the requested Market component on the canvas.
 */
export async function handleAddMarketComponentToCanvas(
  params: AddMarketComponentParams
): Promise<InsertMarketComponentResult> {
  const component = marketRegistry.getById(params.component_id);
  if (!component) {
    throw new Error(`Market component "${params.component_id}" not found in registry.`);
  }

  return await insertMarketComponentToCanvas(component, {
    x: params.position?.x,
    y: params.position?.y,
    parentFrameId: params.parent_frame_id,
    customText: params.custom_text,
    accentColor: params.accent_color,
  });
}

/**
 * Unified tool dispatcher for Market AI tools
 */
export async function executeAiMarketTool(toolName: string, args: Record<string, any>): Promise<any> {
  switch (toolName) {
    case 'search_market_components':
      return await handleSearchMarketComponents(args as SearchMarketParams);
    case 'add_market_component_to_canvas':
      return await handleAddMarketComponentToCanvas(args as AddMarketComponentParams);
    default:
      throw new Error(`Unknown AI market tool: ${toolName}`);
  }
}

// =========================================================================
// 3. AI System Instructions Snippet (Priority #1 Rule)
// =========================================================================

export const AI_MARKET_PRIORITY_INSTRUCTIONS = `
# Component Marketplace Priority (#1 Rule for Generating UI)

When the user asks to build or design any UI screen, feature, or element (e.g., "create a dashboard", "add a login form", "make a kanban board", "add buttons", "create a pricing table", "add charts"):
1. FIRST PRIORITY: Always search and pick matching prebuilt components from the **Market Registry** (@shadcn, @originui, @kibo, @cult, @animate-ui, @bundui, @neobrutalism) using \`search_market_components\`.
2. CUSTOMIZATION: Use \`add_market_component_to_canvas\` to instantiate the picked component, customizing titles, labels, numbers, and color themes to match the user's request.
3. FALLBACK ONLY: Only if no suitable component exists in the Market Registry should you generate custom primitive elements from scratch.
`;
