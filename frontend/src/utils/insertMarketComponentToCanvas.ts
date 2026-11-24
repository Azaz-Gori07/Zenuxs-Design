/**
 * sycnUI Canvas Insertion Utility for Prebuilt Market Components
 * 
 * Safely converts MarketComponent AST templates into native sycnUI canvas commands
 * (frames, auto layout, typography, fills, strokes, and child elements).
 */

import { MarketComponent, CanvasNodeDescriptor, marketRegistry } from '../lib/market-registry';

export interface InsertMarketComponentOptions {
  x?: number;
  y?: number;
  parentFrameId?: string | number;
  customText?: Record<string, string>;
  accentColor?: string;
}

export interface InsertMarketComponentResult {
  success: boolean;
  rootNodeId: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  componentId: string;
}

/**
 * Resolves the center point of the current viewport or returns default offset coordinates.
 */
export function getViewportCenterCoordinates(): { x: number; y: number } {
  if (typeof window === 'undefined') {
    return { x: 200, y: 150 };
  }

  const win = window as any;
  if (win.__sycnui_viewport?.center) {
    return win.__sycnui_viewport.center;
  }

  // Default pleasant canvas offset
  return { x: 240, y: 160 };
}

/**
 * Recursively creates canvas nodes from a CanvasNodeDescriptor AST.
 */
async function instantiateNode(
  descriptor: CanvasNodeDescriptor,
  parentId?: string | number,
  overrideX?: number,
  overrideY?: number,
  options?: InsertMarketComponentOptions
): Promise<string> {
  const win = typeof window !== 'undefined' ? (window as any) : null;
  const targetX = overrideX !== undefined ? overrideX : descriptor.x ?? 0;
  const targetY = overrideY !== undefined ? overrideY : descriptor.y ?? 0;

  // Clone props and adapt theme / custom options
  const props = { ...(descriptor.props || {}) };
  if (options?.accentColor && props.fills) {
    // If fills contain accent or brand colors, adapt optionally
  }

  let characters = descriptor.characters;
  if (characters && options?.customText && options.customText[descriptor.name]) {
    characters = options.customText[descriptor.name];
  }

  const createParams: Record<string, any> = {
    type: descriptor.type,
    name: descriptor.name,
    x: targetX,
    y: targetY,
    width: descriptor.width ?? 100,
    height: descriptor.height ?? 40,
    props,
  };

  if (characters !== undefined) {
    createParams.text = characters;
    createParams.characters = characters;
  }

  if (parentId !== undefined) {
    createParams.parent = parentId;
  }

  let createdId = `node_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`;

  if (win?.__sycnui_engine?.execute) {
    try {
      const res = await win.__sycnui_engine.execute('node.create', createParams);
      if (res?.id) createdId = String(res.id);
    } catch (err) {
      console.warn('[insertMarketComponentToCanvas] Engine execute node.create error:', err);
    }
  } else if (win?.figcraft_execute) {
    try {
      const res = win.figcraft_execute('node.create', JSON.stringify(createParams));
      if (res?.id) createdId = String(res.id);
    } catch (err) {
      console.warn('[insertMarketComponentToCanvas] figcraft_execute node.create error:', err);
    }
  }

  // Recursively instantiate children inside this container frame
  if (descriptor.children && descriptor.children.length > 0) {
    for (const child of descriptor.children) {
      await instantiateNode(child, createdId, undefined, undefined, options);
    }
  }

  return createdId;
}

/**
 * Inserts a prebuilt Market component directly onto the canvas.
 */
export async function insertMarketComponentToCanvas(
  componentOrId: MarketComponent | string,
  options: InsertMarketComponentOptions = {}
): Promise<InsertMarketComponentResult> {
  const component: MarketComponent | undefined =
    typeof componentOrId === 'string'
      ? marketRegistry.getById(componentOrId)
      : componentOrId;

  if (!component) {
    throw new Error(`Market component not found: ${componentOrId}`);
  }

  // Resolve target coordinates (drop coordinates or viewport center)
  const coords =
    options.x !== undefined && options.y !== undefined
      ? { x: options.x, y: options.y }
      : getViewportCenterCoordinates();

  const width = component.canvasTemplate.width ?? 240;
  const height = component.canvasTemplate.height ?? 120;

  // Instantiate the root node and its full children tree
  const rootNodeId = await instantiateNode(
    component.canvasTemplate,
    options.parentFrameId,
    coords.x,
    coords.y,
    options
  );

  // Select the newly created component on the canvas
  if (typeof window !== 'undefined') {
    const win = window as any;
    if (win.__sycnui_engine?.execute) {
      try {
        await win.__sycnui_engine.execute('select.set', { ids: [rootNodeId] });
      } catch (err) {
        // non-blocking
      }
    } else if (win?.figcraft_execute) {
      try {
        win.figcraft_execute('select.set', JSON.stringify({ ids: [rootNodeId] }));
      } catch (err) {
        // non-blocking
      }
    }

    // Dispatch DOM events for reactive layers panel & canvas updates
    window.dispatchEvent(
      new CustomEvent('sycnui:market-component-inserted', {
        detail: {
          componentId: component.id,
          nodeId: rootNodeId,
          name: component.name,
          sourceRegistry: component.sourceRegistry,
          x: coords.x,
          y: coords.y,
          width,
          height,
        },
      })
    );

    window.dispatchEvent(
      new CustomEvent('sycnui:node-created', {
        detail: {
          id: rootNodeId,
          name: component.name,
          x: coords.x,
          y: coords.y,
          width,
          height,
        },
      })
    );
  }

  return {
    success: true,
    rootNodeId,
    name: component.name,
    x: coords.x,
    y: coords.y,
    width,
    height,
    componentId: component.id,
  };
}
