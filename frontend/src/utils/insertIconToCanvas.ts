/**
 * sycnUI Canvas Insertion Utility
 * Safely parses, formats, and inserts SVG icons onto the design canvas.
 */

export interface InsertIconOptions {
  color?: string;
  size?: number;
  parentFrameId?: string | number;
  layoutPosition?: 'center' | 'top-left' | 'top-right' | 'bottom-left' | 'bottom-right' | 'after-text';
  name?: string;
}

export interface InsertIconResult {
  success: boolean;
  nodeId: string;
  name: string;
  x: number;
  y: number;
  width: number;
  height: number;
  dataUri: string;
}

/**
 * Safely encodes a UTF-8 SVG string to Base64 without crashing on Unicode characters.
 * Replaces standard btoa() which throws on multibyte characters.
 */
export function safeSvgToBase64(svgString: string): string {
  try {
    return btoa(unescape(encodeURIComponent(svgString)));
  } catch (err) {
    // Fallback for Node.js environments or edge cases
    if (typeof Buffer !== 'undefined') {
      return Buffer.from(svgString, 'utf-8').toString('base64');
    }
    throw err;
  }
}

/**
 * Dynamically resolves the current theme's primary text / foreground color
 * from CSS variables or theme mode.
 */
export function getCanvasThemeTextColor(): string {
  if (typeof window === 'undefined') return '#1e1e1e';

  // 1. Try CSS variables on document element
  const style = window.getComputedStyle(document.documentElement);
  const cssVarColor =
    style.getPropertyValue('--foreground').trim() ||
    style.getPropertyValue('--text-primary').trim() ||
    style.getPropertyValue('--canvas-text').trim();

  if (cssVarColor) return cssVarColor;

  // 2. Fallback to dark/light detection
  const isDarkMode =
    document.documentElement.classList.contains('dark') ||
    window.matchMedia('(prefers-color-scheme: dark)').matches;

  return isDarkMode ? '#f5f5f5' : '#1e1e1e';
}

/**
 * Inserts an SVG icon onto the SycnUI canvas engine.
 * 
 * @param svgString Raw SVG string from Iconify or asset library
 * @param x Desired X coordinate on canvas (optional if layoutPosition provided)
 * @param y Desired Y coordinate on canvas (optional if layoutPosition provided)
 * @param options Color, sizing, and container placement options
 * @returns Insertion result containing the created node ID for UI selection
 */
export async function insertIconToCanvas(
  svgString: string,
  x?: number,
  y?: number,
  options: InsertIconOptions = {}
): Promise<InsertIconResult> {
  const size = options.size || 24;
  const resolvedColor = options.color || getCanvasThemeTextColor();
  const iconName = options.name || 'Icon';

  // 1. Prepare SVG: ensure xmlns, dimensions, and replace currentColor dynamically
  let preparedSvg = svgString.trim();

  if (!preparedSvg.includes('xmlns=')) {
    preparedSvg = preparedSvg.replace('<svg', '<svg xmlns="http://www.w3.org/2000/svg"');
  }

  // Replace currentColor with resolved theme color
  preparedSvg = preparedSvg.replace(/currentColor/g, resolvedColor);

  // Set explicit width and height if not present
  if (!preparedSvg.includes('width=') && !preparedSvg.includes('height=')) {
    preparedSvg = preparedSvg.replace('<svg', `<svg width="${size}" height="${size}"`);
  }

  // 2. Safely encode to base64 Data URI
  const base64Data = safeSvgToBase64(preparedSvg);
  const dataUri = `data:image/svg+xml;base64,${base64Data}`;

  // 3. Compute target coordinates
  let targetX = x ?? 100;
  let targetY = y ?? 100;

  if (options.layoutPosition) {
    switch (options.layoutPosition) {
      case 'top-left':
        targetX = 16;
        targetY = 16;
        break;
      case 'top-right':
        targetX = 200;
        targetY = 16;
        break;
      case 'center':
        targetX = 100;
        targetY = 100;
        break;
      case 'bottom-left':
        targetX = 16;
        targetY = 200;
        break;
      case 'bottom-right':
        targetX = 200;
        targetY = 200;
        break;
      case 'after-text':
        targetX = 120;
        targetY = 24;
        break;
    }
  }

  // 4. Generate unique node ID for tracking and canvas highlighting
  const generatedId = `node_${Date.now()}_${Math.random().toString(36).substring(2, 7)}`;
  let finalNodeId = generatedId;

  // 5. Dispatch command to sycnUI canvas engine (WebAssembly bridge / WebSocket / CustomEvent)
  if (typeof window !== 'undefined') {
    const win = window as any;

    const commandPayload = {
      command: 'node.create',
      params: {
        type: 'frame',
        name: iconName,
        x: targetX,
        y: targetY,
        width: size,
        height: size,
        parent: options.parentFrameId,
        props: {
          svgDataUri: dataUri,
          rawSvg: preparedSvg,
          fills: [{ type: 'solid', color: `${resolvedColor}15` }],
          stroke: resolvedColor,
          cornerRadius: 4,
        },
      },
    };

    // If native sycnUI / figcraft engine API is exposed on window
    if (win.__sycnui_engine?.execute) {
      try {
        const res = await win.__sycnui_engine.execute(commandPayload.command, commandPayload.params);
        if (res?.id) finalNodeId = String(res.id);
      } catch (e) {
        console.warn('[insertIconToCanvas] Engine execute call error:', e);
      }
    } else if (win.figcraft_execute) {
      try {
        const res = win.figcraft_execute(commandPayload.command, JSON.stringify(commandPayload.params));
        if (res?.id) finalNodeId = String(res.id);
      } catch (e) {
        console.warn('[insertIconToCanvas] figcraft_execute call error:', e);
      }
    }

    // Dispatch DOM event for reactive UI layers / selection update
    window.dispatchEvent(
      new CustomEvent('sycnui:node-created', {
        detail: {
          id: finalNodeId,
          name: iconName,
          x: targetX,
          y: targetY,
          width: size,
          height: size,
          dataUri,
        },
      })
    );
  }

  return {
    success: true,
    nodeId: finalNodeId,
    name: iconName,
    x: targetX,
    y: targetY,
    width: size,
    height: size,
    dataUri,
  };
}
