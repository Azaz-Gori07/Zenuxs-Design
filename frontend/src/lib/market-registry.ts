/**
 * sycnUI Unified Component Marketplace Registry Service
 * 
 * Provides unified fetching, indexing, caching, and schema normalization
 * across 100% Free Shadcn Ecosystem Public Registries:
 * - @shadcn
 * - @originui
 * - @kibo
 * - @cult
 * - @animate-ui
 * - @bundui
 * - @neobrutalism
 * 
 * No API keys or user logins required.
 */

export type MarketCategory =
  | 'All'
  | 'Buttons'
  | 'Inputs'
  | 'Cards'
  | 'Charts'
  | 'Kanban/Complex'
  | 'Animations';

export type MarketRegistrySource =
  | '@shadcn'
  | '@originui'
  | '@kibo'
  | '@cult'
  | '@animate-ui'
  | '@bundui'
  | '@neobrutalism';

export interface CanvasNodeDescriptor {
  type: 'frame' | 'text' | 'rectangle' | 'ellipse' | 'vector';
  name: string;
  x?: number;
  y?: number;
  width?: number;
  height?: number;
  characters?: string;
  props?: {
    layout?: 'vertical' | 'horizontal' | 'none';
    layoutMode?: 'vertical' | 'horizontal' | 'none';
    itemSpacing?: number;
    padding?: number | [number, number, number, number];
    paddingTop?: number;
    paddingRight?: number;
    paddingBottom?: number;
    paddingLeft?: number;
    fills?: Array<{ type: string; color?: string; opacity?: number }>;
    strokes?: Array<{ type: string; color?: string; width?: number }>;
    strokeWeight?: number;
    stroke?: string;
    cornerRadius?: number | [number, number, number, number];
    fontSize?: number;
    fontFamily?: string;
    fontWeight?: number | string;
    textAlign?: 'left' | 'center' | 'right';
    sizingH?: 'fixed' | 'fill' | 'hug';
    sizingV?: 'fixed' | 'fill' | 'hug';
    opacity?: number;
    svgDataUri?: string;
    rawSvg?: string;
    effects?: Array<{ type: string; color?: string; offset?: [number, number]; radius?: number; spread?: number }>;
  };
  children?: CanvasNodeDescriptor[];
}

export interface MarketComponent {
  id: string;
  name: string;
  sourceRegistry: MarketRegistrySource;
  category: Exclude<MarketCategory, 'All'>;
  description: string;
  tags: string[];
  code: string;
  dependencies: string[];
  previewData: {
    thumbnailSvg?: string;
    badgeText?: string;
    accentColor?: string;
    width?: number;
    height?: number;
  };
  canvasTemplate: CanvasNodeDescriptor;
}

export interface RegistryConfig {
  source: MarketRegistrySource;
  label: string;
  baseUrl: string;
  badgeColor: string;
  description: string;
}

export const REGISTRY_CONFIGS: Record<MarketRegistrySource, RegistryConfig> = {
  '@shadcn': {
    source: '@shadcn',
    label: 'shadcn/ui',
    baseUrl: 'https://ui.shadcn.com/r/{name}.json',
    badgeColor: '#18181b',
    description: 'The standard modern accessible component primitives for React & Tailwind',
  },
  '@originui': {
    source: '@originui',
    label: 'Origin UI',
    baseUrl: 'https://originui.com/r/{name}.json',
    badgeColor: '#2563eb',
    description: 'Polished interactive inputs, buttons, and micro-interactions',
  },
  '@kibo': {
    source: '@kibo',
    label: 'Kibo UI',
    baseUrl: 'https://www.kibo-ui.com/r/{name}.json',
    badgeColor: '#7c3aed',
    description: 'Advanced data tables, kanban boards, and enterprise widgets',
  },
  '@cult': {
    source: '@cult',
    label: 'Cult UI',
    baseUrl: 'https://www.cult-ui.com/r/{name}.json',
    badgeColor: '#ec4899',
    description: 'Cyberpunk, glowing, and high-impact visual design components',
  },
  '@animate-ui': {
    source: '@animate-ui',
    label: 'Animate UI',
    baseUrl: 'https://animate-ui.com/r/{name}.json',
    badgeColor: '#059669',
    description: 'Smooth spring animations, spotlights, marquees, and gesture physics',
  },
  '@bundui': {
    source: '@bundui',
    label: 'Bund UI',
    baseUrl: 'https://bundui.io/r/{name}.json',
    badgeColor: '#ea580c',
    description: 'Full-featured marketing cards, testimonials, and settings layouts',
  },
  '@neobrutalism': {
    source: '@neobrutalism',
    label: 'Neobrutalism',
    baseUrl: 'https://www.neobrutalism.dev/r/{name}.json',
    badgeColor: '#eab308',
    description: 'Bold retro-brutalist cards, buttons, and high-contrast UI blocks',
  },
};

// =========================================================================
// Curated 100% Free Prebuilt Market Registry Index (Instant / Offline Ready)
// =========================================================================

export const PREBUILT_MARKET_COMPONENTS: MarketComponent[] = [
  // -----------------------------------------------------------------------
  // 1. BUTTONS
  // -----------------------------------------------------------------------
  {
    id: '@shadcn/button',
    name: 'Primary & Secondary Buttons',
    sourceRegistry: '@shadcn',
    category: 'Buttons',
    description: 'Accessible interactive buttons with focus rings, hover states, and variant styling.',
    tags: ['button', 'action', 'cta', 'interactive', 'primary', 'shadcn'],
    dependencies: ['@radix-ui/react-slot', 'class-variance-authority', 'clsx', 'tailwind-merge'],
    code: `import * as React from "react"
import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"
import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-md text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground shadow hover:bg-primary/90",
        destructive: "bg-destructive text-destructive-foreground shadow-sm hover:bg-destructive/90",
        outline: "border border-input bg-background shadow-sm hover:bg-accent hover:text-accent-foreground",
        secondary: "bg-secondary text-secondary-foreground shadow-sm hover:bg-secondary/80",
        ghost: "hover:bg-accent hover:text-accent-foreground",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default: "h-9 px-4 py-2",
        sm: "h-8 rounded-md px-3 text-xs",
        lg: "h-10 rounded-md px-8",
        icon: "h-9 w-9",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

export function Button({ className, variant, size, ...props }) {
  return <button className={cn(buttonVariants({ variant, size, className }))} {...props} />
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#18181b',
      width: 140,
      height: 40,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="140" height="40" viewBox="0 0 140 40"><rect width="140" height="40" rx="8" fill="#18181b"/><text x="70" y="24" fill="#ffffff" font-family="sans-serif" font-size="13" font-weight="600" text-anchor="middle">Get Started →</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Button / Primary',
      width: 140,
      height: 40,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 8,
        padding: [10, 18, 10, 18],
        cornerRadius: 8,
        fills: [{ type: 'solid', color: '#18181b' }],
      },
      children: [
        {
          type: 'text',
          name: 'Label',
          characters: 'Get Started',
          props: {
            fontSize: 14,
            fontWeight: 600,
            fills: [{ type: 'solid', color: '#ffffff' }],
          },
        },
      ],
    },
  },
  {
    id: '@originui/button-shimmer',
    name: 'Shimmer Border Button',
    sourceRegistry: '@originui',
    category: 'Buttons',
    description: 'High-polish interactive button featuring a flowing gradient shine border and subtle glow.',
    tags: ['button', 'shimmer', 'gradient', 'originui', 'modern', 'glow'],
    dependencies: ['framer-motion', 'clsx', 'tailwind-merge'],
    code: `export function ShimmerButton({ children = "Explore Features" }) {
  return (
    <button className="relative inline-flex h-10 overflow-hidden rounded-lg p-[1px] focus:outline-none focus:ring-2 focus:ring-slate-400 focus:ring-offset-2">
      <span className="absolute inset-[-1000%] animate-[spin_2s_linear_infinite] bg-[conic-gradient(from_90deg_at_50%_50%,#E2CBFF_0%,#393BB2_50%,#E2CBFF_100%)]" />
      <span className="inline-flex h-full w-full cursor-pointer items-center justify-center rounded-lg bg-slate-950 px-5 py-1 text-sm font-medium text-white backdrop-blur-3xl hover:bg-slate-900 transition-colors">
        {children}
      </span>
    </button>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#3b82f6',
      width: 150,
      height: 42,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="150" height="42" viewBox="0 0 150 42"><rect x="1" y="1" width="148" height="40" rx="9" fill="#090d16" stroke="#3b82f6" stroke-width="2"/><text x="75" y="25" fill="#60a5fa" font-family="sans-serif" font-size="13" font-weight="600" text-anchor="middle">✨ Explore Features</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Button / Shimmer',
      width: 154,
      height: 42,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 8,
        padding: [11, 20, 11, 20],
        cornerRadius: 10,
        fills: [{ type: 'solid', color: '#090d16' }],
        strokes: [{ type: 'solid', color: '#3b82f6', width: 1.5 }],
      },
      children: [
        {
          type: 'text',
          name: 'Label',
          characters: '✨ Explore Features',
          props: {
            fontSize: 14,
            fontWeight: 600,
            fills: [{ type: 'solid', color: '#93c5fd' }],
          },
        },
      ],
    },
  },
  {
    id: '@neobrutalism/neo-button',
    name: 'Neo-Brutalist Tactile Button',
    sourceRegistry: '@neobrutalism',
    category: 'Buttons',
    description: 'High-contrast retro button with 2.5px solid border, vibrant background, and hard drop shadow.',
    tags: ['button', 'neobrutalism', 'retro', 'brutalist', 'high-contrast'],
    dependencies: ['clsx', 'tailwind-merge'],
    code: `export function NeoButton({ children = "Click Me!" }) {
  return (
    <button className="px-5 py-2.5 bg-[#FEF08A] text-black font-bold text-sm border-2 border-black rounded-md shadow-[4px_4px_0px_0px_rgba(0,0,0,1)] active:translate-x-[2px] active:translate-y-[2px] active:shadow-[2px_2px_0px_0px_rgba(0,0,0,1)] transition-all">
      {children}
    </button>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#facc15',
      width: 130,
      height: 42,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="130" height="42" viewBox="0 0 130 42"><rect x="4" y="4" width="122" height="34" rx="4" fill="#000000"/><rect x="0" y="0" width="122" height="34" rx="4" fill="#fde047" stroke="#000000" stroke-width="2.5"/><text x="61" y="22" fill="#000000" font-family="sans-serif" font-size="13" font-weight="700" text-anchor="middle">Neo Action ⚡</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Button / Neobrutalism',
      width: 136,
      height: 42,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 6,
        padding: [10, 16, 10, 16],
        cornerRadius: 6,
        fills: [{ type: 'solid', color: '#fde047' }],
        strokes: [{ type: 'solid', color: '#000000', width: 2.5 }],
        effects: [{ type: 'drop-shadow', color: '#000000', offset: [4, 4], radius: 0 }],
      },
      children: [
        {
          type: 'text',
          name: 'Label',
          characters: 'Neo Action ⚡',
          props: {
            fontSize: 14,
            fontWeight: 700,
            fills: [{ type: 'solid', color: '#000000' }],
          },
        },
      ],
    },
  },
  {
    id: '@cult/glow-button',
    name: 'Cyberpunk Neon Glow Button',
    sourceRegistry: '@cult',
    category: 'Buttons',
    description: 'Vibrant neon illuminated button with pulsing radial backdrops and sleek dark background.',
    tags: ['button', 'glow', 'cyberpunk', 'cult', 'neon', 'futuristic'],
    dependencies: ['framer-motion'],
    code: `export function GlowButton({ children = "Launch Mission" }) {
  return (
    <div className="relative group inline-block">
      <div className="absolute -inset-0.5 bg-gradient-to-r from-pink-600 to-purple-600 rounded-lg blur opacity-75 group-hover:opacity-100 transition duration-1000 group-hover:duration-200 animate-tilt"></div>
      <button className="relative px-6 py-2.5 bg-black rounded-lg text-sm font-semibold text-white leading-none flex items-center divide-x divide-gray-600">
        <span className="pr-4">{children}</span>
        <span className="pl-4 text-pink-400 group-hover:text-gray-100 transition duration-200">&rarr;</span>
      </button>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#ec4899',
      width: 150,
      height: 42,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="150" height="42" viewBox="0 0 150 42"><rect x="0" y="0" width="150" height="42" rx="8" fill="#180521" stroke="#ec4899" stroke-width="1.5"/><text x="75" y="25" fill="#f472b6" font-family="sans-serif" font-size="13" font-weight="600" text-anchor="middle">Launch Mission →</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Button / Cyber Glow',
      width: 160,
      height: 42,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 8,
        padding: [10, 20, 10, 20],
        cornerRadius: 8,
        fills: [{ type: 'solid', color: '#13041c' }],
        strokes: [{ type: 'solid', color: '#ec4899', width: 1.5 }],
        effects: [{ type: 'drop-shadow', color: '#ec489955', offset: [0, 0], radius: 12 }],
      },
      children: [
        {
          type: 'text',
          name: 'Label',
          characters: 'Launch Mission →',
          props: {
            fontSize: 14,
            fontWeight: 600,
            fills: [{ type: 'solid', color: '#f472b6' }],
          },
        },
      ],
    },
  },

  // -----------------------------------------------------------------------
  // 2. INPUTS
  // -----------------------------------------------------------------------
  {
    id: '@shadcn/input',
    name: 'Standard Text Input',
    sourceRegistry: '@shadcn',
    category: 'Inputs',
    description: 'Clean accessible text input with subtle border, focus ring, and placeholder styling.',
    tags: ['input', 'form', 'text', 'field', 'shadcn'],
    dependencies: ['clsx', 'tailwind-merge'],
    code: `export function Input({ type = "text", placeholder = "Enter email address...", ...props }) {
  return (
    <input
      type={type}
      placeholder={placeholder}
      className="flex h-10 w-full rounded-md border border-input bg-background px-3 py-2 text-sm ring-offset-background file:border-0 file:bg-transparent file:text-sm file:font-medium placeholder:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2 disabled:cursor-not-allowed disabled:opacity-50"
      {...props}
    />
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#64748b',
      width: 220,
      height: 40,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="220" height="40" viewBox="0 0 220 40"><rect width="220" height="40" rx="6" fill="#ffffff" stroke="#cbd5e1" stroke-width="1.5"/><text x="14" y="24" fill="#94a3b8" font-family="sans-serif" font-size="13">Enter email address...</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Input / Text Field',
      width: 240,
      height: 42,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        padding: [10, 14, 10, 14],
        cornerRadius: 6,
        fills: [{ type: 'solid', color: '#ffffff' }],
        strokes: [{ type: 'solid', color: '#cbd5e1', width: 1.5 }],
      },
      children: [
        {
          type: 'text',
          name: 'Placeholder',
          characters: 'Enter email address...',
          props: {
            fontSize: 13,
            fontWeight: 400,
            fills: [{ type: 'solid', color: '#94a3b8' }],
          },
        },
      ],
    },
  },
  {
    id: '@originui/input-search-kbd',
    name: 'Search Bar with Keyboard Shortcut',
    sourceRegistry: '@originui',
    category: 'Inputs',
    description: 'Search input integrated with search icon prefix and dynamic ⌘K keyboard shortcut indicator.',
    tags: ['search', 'input', 'shortcut', 'kbd', 'originui'],
    dependencies: ['lucide-react', 'clsx', 'tailwind-merge'],
    code: `import { Search } from "lucide-react";

export function SearchInput({ placeholder = "Search documentation..." }) {
  return (
    <div className="relative flex items-center w-full max-w-sm">
      <Search className="absolute left-3 w-4 h-4 text-muted-foreground pointer-events-none" />
      <input
        type="search"
        placeholder={placeholder}
        className="w-full h-10 pl-9 pr-14 rounded-lg border border-input bg-background text-sm placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary"
      />
      <div className="absolute right-2.5 flex items-center gap-1">
        <kbd className="px-1.5 py-0.5 text-[10px] font-semibold text-muted-foreground bg-muted border border-border rounded">⌘K</kbd>
      </div>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#3b82f6',
      width: 260,
      height: 42,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="260" height="42" viewBox="0 0 260 42"><rect width="260" height="42" rx="8" fill="#ffffff" stroke="#e2e8f0" stroke-width="1.5"/><circle cx="20" cy="21" r="5" stroke="#94a3b8" stroke-width="1.5" fill="none"/><line x1="24" y1="25" x2="28" y2="29" stroke="#94a3b8" stroke-width="1.5"/><text x="36" y="25" fill="#94a3b8" font-family="sans-serif" font-size="13">Search components...</text><rect x="220" y="11" width="28" height="20" rx="4" fill="#f1f5f9" stroke="#cbd5e1"/><text x="234" y="25" fill="#64748b" font-family="sans-serif" font-size="11" font-weight="600" text-anchor="middle">⌘K</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Input / Search with ⌘K',
      width: 280,
      height: 44,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 10,
        padding: [10, 14, 10, 14],
        cornerRadius: 8,
        fills: [{ type: 'solid', color: '#ffffff' }],
        strokes: [{ type: 'solid', color: '#e2e8f0', width: 1.5 }],
      },
      children: [
        {
          type: 'text',
          name: 'Search Icon',
          characters: '🔍',
          props: { fontSize: 13 },
        },
        {
          type: 'text',
          name: 'Placeholder',
          characters: 'Search components...',
          props: {
            fontSize: 13,
            fontWeight: 400,
            fills: [{ type: 'solid', color: '#94a3b8' }],
            sizingH: 'fill',
          },
        },
        {
          type: 'frame',
          name: 'Badge ⌘K',
          width: 32,
          height: 22,
          props: {
            layout: 'horizontal',
            padding: [2, 6, 2, 6],
            cornerRadius: 4,
            fills: [{ type: 'solid', color: '#f1f5f9' }],
            strokes: [{ type: 'solid', color: '#cbd5e1', width: 1 }],
          },
          children: [
            {
              type: 'text',
              name: 'Kbd',
              characters: '⌘K',
              props: { fontSize: 11, fontWeight: 600, fills: [{ type: 'solid', color: '#64748b' }] },
            },
          ],
        },
      ],
    },
  },
  {
    id: '@kibo/tags-input',
    name: 'Multi-Tag Filter Input',
    sourceRegistry: '@kibo',
    category: 'Inputs',
    description: 'Interactive chips container allowing users to add, select, and dismiss tags in form fields.',
    tags: ['tags', 'chips', 'input', 'multiselect', 'kibo'],
    dependencies: ['clsx', 'lucide-react'],
    code: `import { X } from "lucide-react";

export function TagsInput({ initialTags = ["React", "Tailwind", "Rust"] }) {
  return (
    <div className="flex flex-wrap items-center gap-1.5 p-2 rounded-lg border border-input bg-background min-h-11">
      {initialTags.map(tag => (
        <span key={tag} className="inline-flex items-center gap-1 px-2.5 py-1 rounded-md bg-secondary text-secondary-foreground text-xs font-medium">
          {tag}
          <button className="text-muted-foreground hover:text-foreground"><X className="w-3 h-3" /></button>
        </span>
      ))}
      <input type="text" placeholder="Add tag..." className="flex-1 bg-transparent text-xs outline-none px-1 py-1" />
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#7c3aed',
      width: 260,
      height: 44,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="260" height="44" viewBox="0 0 260 44"><rect width="260" height="44" rx="8" fill="#ffffff" stroke="#e2e8f0"/><rect x="8" y="9" width="60" height="26" rx="5" fill="#f3e8ff"/><text x="32" y="26" fill="#7e22ce" font-family="sans-serif" font-size="12" font-weight="600">React ✕</text><rect x="74" y="9" width="75" height="26" rx="5" fill="#e0e7ff"/><text x="105" y="26" fill="#4338ca" font-family="sans-serif" font-size="12" font-weight="600">Tailwind ✕</text><text x="160" y="26" fill="#94a3b8" font-family="sans-serif" font-size="12">Add tag...</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Input / Tag Chips',
      width: 280,
      height: 48,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 8,
        padding: [8, 12, 8, 12],
        cornerRadius: 8,
        fills: [{ type: 'solid', color: '#ffffff' }],
        strokes: [{ type: 'solid', color: '#e2e8f0', width: 1.5 }],
      },
      children: [
        {
          type: 'frame',
          name: 'Tag / React',
          width: 70,
          height: 28,
          props: {
            cornerRadius: 6,
            fills: [{ type: 'solid', color: '#f3e8ff' }],
            padding: [4, 8, 4, 8],
          },
          children: [
            {
              type: 'text',
              name: 'Tag Text',
              characters: 'React ✕',
              props: { fontSize: 12, fontWeight: 600, fills: [{ type: 'solid', color: '#7e22ce' }] },
            },
          ],
        },
        {
          type: 'frame',
          name: 'Tag / Tailwind',
          width: 84,
          height: 28,
          props: {
            cornerRadius: 6,
            fills: [{ type: 'solid', color: '#e0e7ff' }],
            padding: [4, 8, 4, 8],
          },
          children: [
            {
              type: 'text',
              name: 'Tag Text',
              characters: 'Tailwind ✕',
              props: { fontSize: 12, fontWeight: 600, fills: [{ type: 'solid', color: '#4338ca' }] },
            },
          ],
        },
        {
          type: 'text',
          name: 'Input Prompt',
          characters: 'Add tag...',
          props: { fontSize: 12, fills: [{ type: 'solid', color: '#94a3b8' }] },
        },
      ],
    },
  },

  // -----------------------------------------------------------------------
  // 3. CARDS
  // -----------------------------------------------------------------------
  {
    id: '@shadcn/card-metric',
    name: 'Metric KPI Stat Card',
    sourceRegistry: '@shadcn',
    category: 'Cards',
    description: 'Executive dashboard KPI metric card with title, large numeric value, and percentage trend pill.',
    tags: ['card', 'metric', 'stat', 'kpi', 'dashboard', 'shadcn'],
    dependencies: ['lucide-react', 'clsx', 'tailwind-merge'],
    code: `import { TrendingUp } from "lucide-react";

export function MetricCard({ title = "Total Revenue", value = "$45,231.89", trend = "+20.1% from last month" }) {
  return (
    <div className="rounded-xl border bg-card text-card-foreground shadow p-6 max-w-sm">
      <div className="flex items-center justify-between space-y-0 pb-2">
        <p className="text-sm font-medium text-muted-foreground">{title}</p>
        <span className="p-1 rounded-full bg-emerald-100 dark:bg-emerald-950 text-emerald-600"><TrendingUp className="w-4 h-4" /></span>
      </div>
      <div className="text-2xl font-bold">{value}</div>
      <p className="text-xs text-muted-foreground mt-1 text-emerald-600 font-medium">{trend}</p>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#10b981',
      width: 240,
      height: 120,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120" viewBox="0 0 240 120"><rect width="240" height="120" rx="12" fill="#ffffff" stroke="#e2e8f0" stroke-width="1.5"/><text x="16" y="32" fill="#64748b" font-family="sans-serif" font-size="13" font-weight="500">Total Revenue</text><circle cx="216" cy="28" r="12" fill="#d1fae5"/><text x="16" y="70" fill="#0f172a" font-family="sans-serif" font-size="24" font-weight="700">$45,231.89</text><text x="16" y="98" fill="#059669" font-family="sans-serif" font-size="12" font-weight="600">+20.1% from last month</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Card / Metric KPI',
      width: 260,
      height: 130,
      props: {
        layout: 'vertical',
        layoutMode: 'vertical',
        itemSpacing: 8,
        padding: [18, 20, 18, 20],
        cornerRadius: 12,
        fills: [{ type: 'solid', color: '#ffffff' }],
        strokes: [{ type: 'solid', color: '#e2e8f0', width: 1.5 }],
        effects: [{ type: 'drop-shadow', color: '#00000008', offset: [0, 4], radius: 10 }],
      },
      children: [
        {
          type: 'frame',
          name: 'Header Row',
          width: 220,
          height: 24,
          props: { layout: 'horizontal', sizingH: 'fill', itemSpacing: 8 },
          children: [
            {
              type: 'text',
              name: 'Title',
              characters: 'Total Revenue',
              props: { fontSize: 13, fontWeight: 500, fills: [{ type: 'solid', color: '#64748b' }] },
            },
          ],
        },
        {
          type: 'text',
          name: 'Metric Value',
          characters: '$45,231.89',
          props: { fontSize: 26, fontWeight: 700, fills: [{ type: 'solid', color: '#0f172a' }] },
        },
        {
          type: 'text',
          name: 'Trend Subtitle',
          characters: '+20.1% from last month',
          props: { fontSize: 12, fontWeight: 600, fills: [{ type: 'solid', color: '#059669' }] },
        },
      ],
    },
  },
  {
    id: '@originui/pricing-card',
    name: 'Tiered Pricing Feature Card',
    sourceRegistry: '@originui',
    category: 'Cards',
    description: 'Highlighted subscription pricing card with "Most Popular" ribbon, feature checklist, and primary CTA.',
    tags: ['pricing', 'card', 'subscription', 'originui', 'marketing'],
    dependencies: ['lucide-react', 'clsx'],
    code: `import { Check } from "lucide-react";

export function PricingCard({ plan = "Pro Plan", price = "$29", period = "/month" }) {
  const features = ["Unlimited projects", "10 team members", "Real-time canvas sync", "Export high-res PDF/SVG", "Priority AI generations"];
  return (
    <div className="relative rounded-2xl border-2 border-indigo-600 bg-card p-6 shadow-xl max-w-sm">
      <div className="absolute -top-3 right-6 px-3 py-1 rounded-full bg-indigo-600 text-[11px] font-semibold text-white tracking-wide uppercase">Most Popular</div>
      <h3 className="text-lg font-bold text-foreground">{plan}</h3>
      <div className="mt-4 flex items-baseline gap-1">
        <span className="text-4xl font-extrabold text-foreground">{price}</span>
        <span className="text-sm text-muted-foreground">{period}</span>
      </div>
      <ul className="mt-6 space-y-3">
        {features.map(f => (
          <li key={f} className="flex items-center gap-2 text-xs text-muted-foreground">
            <span className="p-0.5 rounded-full bg-indigo-100 text-indigo-600"><Check className="w-3 h-3" /></span>
            {f}
          </li>
        ))}
      </ul>
      <button className="mt-6 w-full py-2.5 rounded-xl bg-indigo-600 text-white font-semibold text-sm shadow hover:bg-indigo-700">Get Started</button>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#4f46e5',
      width: 260,
      height: 280,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="260" height="280" viewBox="0 0 260 280"><rect width="260" height="280" rx="16" fill="#ffffff" stroke="#4f46e5" stroke-width="2"/><rect x="150" y="8" width="96" height="20" rx="10" fill="#4f46e5"/><text x="198" y="22" fill="#ffffff" font-family="sans-serif" font-size="10" font-weight="700" text-anchor="middle">MOST POPULAR</text><text x="20" y="44" fill="#0f172a" font-family="sans-serif" font-size="18" font-weight="700">Pro Plan</text><text x="20" y="86" fill="#0f172a" font-family="sans-serif" font-size="36" font-weight="800">$29<tspan font-size="14" font-weight="400" fill="#64748b">/mo</tspan></text><text x="20" y="126" fill="#334155" font-family="sans-serif" font-size="12">✓ Unlimited projects</text><text x="20" y="152" fill="#334155" font-family="sans-serif" font-size="12">✓ 10 team members</text><text x="20" y="178" fill="#334155" font-family="sans-serif" font-size="12">✓ Real-time canvas sync</text><rect x="20" y="220" width="220" height="40" rx="10" fill="#4f46e5"/><text x="130" y="245" fill="#ffffff" font-family="sans-serif" font-size="14" font-weight="600" text-anchor="middle">Get Started</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Card / Pricing Highlight',
      width: 280,
      height: 310,
      props: {
        layout: 'vertical',
        layoutMode: 'vertical',
        itemSpacing: 14,
        padding: [22, 22, 22, 22],
        cornerRadius: 16,
        fills: [{ type: 'solid', color: '#ffffff' }],
        strokes: [{ type: 'solid', color: '#4f46e5', width: 2 }],
        effects: [{ type: 'drop-shadow', color: '#4f46e51f', offset: [0, 8], radius: 20 }],
      },
      children: [
        {
          type: 'frame',
          name: 'Badge Row',
          width: 236,
          height: 24,
          props: { layout: 'horizontal', itemSpacing: 8 },
          children: [
            {
              type: 'text',
              name: 'Plan Name',
              characters: 'Pro Plan',
              props: { fontSize: 18, fontWeight: 700, fills: [{ type: 'solid', color: '#0f172a' }], sizingH: 'fill' },
            },
            {
              type: 'frame',
              name: 'Badge',
              width: 100,
              height: 22,
              props: { cornerRadius: 10, fills: [{ type: 'solid', color: '#4f46e5' }], padding: [3, 8, 3, 8] },
              children: [
                {
                  type: 'text',
                  name: 'Popular',
                  characters: 'MOST POPULAR',
                  props: { fontSize: 10, fontWeight: 700, fills: [{ type: 'solid', color: '#ffffff' }] },
                },
              ],
            },
          ],
        },
        {
          type: 'text',
          name: 'Price',
          characters: '$29 / month',
          props: { fontSize: 32, fontWeight: 800, fills: [{ type: 'solid', color: '#0f172a' }] },
        },
        {
          type: 'text',
          name: 'Feature 1',
          characters: '✓ Unlimited canvas layers & assets',
          props: { fontSize: 12, fontWeight: 500, fills: [{ type: 'solid', color: '#475569' }] },
        },
        {
          type: 'text',
          name: 'Feature 2',
          characters: '✓ Real-time collaboration & sync',
          props: { fontSize: 12, fontWeight: 500, fills: [{ type: 'solid', color: '#475569' }] },
        },
        {
          type: 'text',
          name: 'Feature 3',
          characters: '✓ Priority AI market component solver',
          props: { fontSize: 12, fontWeight: 500, fills: [{ type: 'solid', color: '#475569' }] },
        },
        {
          type: 'frame',
          name: 'CTA Button',
          width: 236,
          height: 42,
          props: {
            layout: 'horizontal',
            layoutMode: 'horizontal',
            cornerRadius: 10,
            fills: [{ type: 'solid', color: '#4f46e5' }],
            padding: [10, 20, 10, 20],
          },
          children: [
            {
              type: 'text',
              name: 'Button Label',
              characters: 'Get Started Now',
              props: { fontSize: 14, fontWeight: 600, fills: [{ type: 'solid', color: '#ffffff' }], textAlign: 'center', sizingH: 'fill' },
            },
          ],
        },
      ],
    },
  },
  {
    id: '@cult/bento-card',
    name: 'Bento Grid Showcase Card',
    sourceRegistry: '@cult',
    category: 'Cards',
    description: 'Modern Apple/Bento-style visual showcase card with subtle gradient background and icon highlight.',
    tags: ['bento', 'card', 'showcase', 'cult', 'gradient', 'apple'],
    dependencies: ['lucide-react'],
    code: `import { Sparkles } from "lucide-react";

export function BentoCard({ title = "Intelligent AI Layouts", description = "Automatic layout engine that aligns and spaces design tokens with mathematical precision." }) {
  return (
    <div className="group relative flex flex-col justify-between overflow-hidden rounded-3xl bg-neutral-900 border border-neutral-800 p-6 hover:border-neutral-700 transition-all duration-300">
      <div className="w-12 h-12 rounded-2xl bg-indigo-500/10 border border-indigo-500/20 flex items-center justify-center text-indigo-400 mb-6">
        <Sparkles className="w-6 h-6" />
      </div>
      <div>
        <h4 className="text-lg font-semibold text-white tracking-tight">{title}</h4>
        <p className="mt-2 text-sm text-neutral-400 leading-relaxed">{description}</p>
      </div>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#6366f1',
      width: 240,
      height: 180,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="180" viewBox="0 0 240 180"><rect width="240" height="180" rx="16" fill="#171717" stroke="#262626"/><rect x="20" y="20" width="40" height="40" rx="10" fill="#312e81"/><text x="40" y="45" fill="#a5b4fc" font-size="18" text-anchor="middle">✦</text><text x="20" y="100" fill="#ffffff" font-family="sans-serif" font-size="15" font-weight="700">Intelligent AI Layouts</text><text x="20" y="126" fill="#a3a3a3" font-family="sans-serif" font-size="11">Automatic layout engine aligning design</text><text x="20" y="144" fill="#a3a3a3" font-family="sans-serif" font-size="11">tokens with mathematical precision.</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Card / Bento Showcase',
      width: 280,
      height: 200,
      props: {
        layout: 'vertical',
        layoutMode: 'vertical',
        itemSpacing: 12,
        padding: [22, 22, 22, 22],
        cornerRadius: 16,
        fills: [{ type: 'solid', color: '#171717' }],
        strokes: [{ type: 'solid', color: '#2e2e2e', width: 1.5 }],
      },
      children: [
        {
          type: 'frame',
          name: 'Icon Box',
          width: 44,
          height: 44,
          props: {
            cornerRadius: 10,
            fills: [{ type: 'solid', color: '#312e81' }],
            strokes: [{ type: 'solid', color: '#4338ca', width: 1 }],
            padding: [10, 10, 10, 10],
          },
          children: [
            {
              type: 'text',
              name: 'Icon Symbol',
              characters: '✦',
              props: { fontSize: 18, fills: [{ type: 'solid', color: '#a5b4fc' }], textAlign: 'center' },
            },
          ],
        },
        {
          type: 'text',
          name: 'Heading',
          characters: 'Intelligent AI Layouts',
          props: { fontSize: 16, fontWeight: 700, fills: [{ type: 'solid', color: '#ffffff' }] },
        },
        {
          type: 'text',
          name: 'Description',
          characters: 'Automatic layout engine that aligns and spaces design tokens with mathematical precision.',
          props: { fontSize: 12, fontWeight: 400, fills: [{ type: 'solid', color: '#a3a3a3' }] },
        },
      ],
    },
  },

  // -----------------------------------------------------------------------
  // 4. CHARTS
  // -----------------------------------------------------------------------
  {
    id: '@shadcn/area-chart',
    name: 'Interactive Revenue Area Chart',
    sourceRegistry: '@shadcn',
    category: 'Charts',
    description: 'Clean area chart card displaying revenue growth curves with gradient fills and date axes.',
    tags: ['chart', 'area', 'analytics', 'revenue', 'graph', 'shadcn'],
    dependencies: ['recharts', 'lucide-react'],
    code: `import { Area, AreaChart, ResponsiveContainer, XAxis, Tooltip } from "recharts";

const data = [
  { month: "Jan", revenue: 18000 },
  { month: "Feb", revenue: 24000 },
  { month: "Mar", revenue: 31000 },
  { month: "Apr", revenue: 29000 },
  { month: "May", revenue: 42000 },
  { month: "Jun", revenue: 54000 },
];

export function RevenueChart() {
  return (
    <div className="p-6 rounded-2xl border bg-card shadow-sm">
      <h3 className="font-bold text-base">Revenue Overview</h3>
      <div className="h-48 mt-4">
        <ResponsiveContainer width="100%" height="100%">
          <AreaChart data={data}>
            <defs>
              <linearGradient id="colorRev" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#4f46e5" stopOpacity={0.8}/>
                <stop offset="95%" stopColor="#4f46e5" stopOpacity={0}/>
              </linearGradient>
            </defs>
            <XAxis dataKey="month" stroke="#94a3b8" />
            <Tooltip />
            <Area type="monotone" dataKey="revenue" stroke="#4f46e5" fillOpacity={1} fill="url(#colorRev)" />
          </AreaChart>
        </ResponsiveContainer>
      </div>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#6366f1',
      width: 280,
      height: 160,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="280" height="160" viewBox="0 0 280 160"><rect width="280" height="160" rx="12" fill="#ffffff" stroke="#e2e8f0"/><text x="18" y="28" fill="#0f172a" font-family="sans-serif" font-size="14" font-weight="700">Revenue Overview</text><path d="M 20 120 Q 70 90, 110 95 T 180 60 T 260 40 L 260 130 L 20 130 Z" fill="#e0e7ff" opacity="0.7"/><path d="M 20 120 Q 70 90, 110 95 T 180 60 T 260 40" fill="none" stroke="#4f46e5" stroke-width="2.5"/><text x="30" y="146" fill="#94a3b8" font-size="10">Jan</text><text x="85" y="146" fill="#94a3b8" font-size="10">Feb</text><text x="140" y="146" fill="#94a3b8" font-size="10">Mar</text><text x="195" y="146" fill="#94a3b8" font-size="10">Apr</text><text x="245" y="146" fill="#94a3b8" font-size="10">May</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Chart / Revenue Area Curve',
      width: 320,
      height: 180,
      props: {
        layout: 'vertical',
        layoutMode: 'vertical',
        itemSpacing: 12,
        padding: [16, 20, 16, 20],
        cornerRadius: 12,
        fills: [{ type: 'solid', color: '#ffffff' }],
        strokes: [{ type: 'solid', color: '#e2e8f0', width: 1.5 }],
      },
      children: [
        {
          type: 'text',
          name: 'Chart Title',
          characters: 'Revenue Growth Overview',
          props: { fontSize: 15, fontWeight: 700, fills: [{ type: 'solid', color: '#0f172a' }] },
        },
        {
          type: 'frame',
          name: 'Chart Bars Container',
          width: 280,
          height: 100,
          props: { layout: 'horizontal', itemSpacing: 14, padding: [10, 10, 0, 10], sizingH: 'fill' },
          children: [
            { type: 'frame', name: 'Bar Jan', width: 28, height: 45, props: { cornerRadius: 4, fills: [{ type: 'solid', color: '#818cf8' }] } },
            { type: 'frame', name: 'Bar Feb', width: 28, height: 60, props: { cornerRadius: 4, fills: [{ type: 'solid', color: '#6366f1' }] } },
            { type: 'frame', name: 'Bar Mar', width: 28, height: 75, props: { cornerRadius: 4, fills: [{ type: 'solid', color: '#4f46e5' }] } },
            { type: 'frame', name: 'Bar Apr', width: 28, height: 70, props: { cornerRadius: 4, fills: [{ type: 'solid', color: '#4338ca' }] } },
            { type: 'frame', name: 'Bar May', width: 28, height: 95, props: { cornerRadius: 4, fills: [{ type: 'solid', color: '#3730a3' }] } },
          ],
        },
      ],
    },
  },
  {
    id: '@originui/donut-chart',
    name: 'Category Distribution Donut',
    sourceRegistry: '@originui',
    category: 'Charts',
    description: 'Circular donut chart visualizing market share breakdown and legend labels.',
    tags: ['donut', 'pie', 'chart', 'analytics', 'distribution', 'originui'],
    dependencies: ['recharts'],
    code: `export function DonutChart() {
  return (
    <div className="p-6 rounded-2xl border bg-card shadow-sm flex items-center justify-between">
      <div>
        <h4 className="font-bold text-sm">Traffic Source</h4>
        <p className="text-xs text-muted-foreground mt-1">Direct: 45% • Referral: 30% • Social: 25%</p>
      </div>
      <div className="w-20 h-20 rounded-full border-4 border-indigo-600 border-t-emerald-500 border-r-amber-500 animate-spin-slow"></div>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#10b981',
      width: 240,
      height: 120,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120" viewBox="0 0 240 120"><rect width="240" height="120" rx="12" fill="#ffffff" stroke="#e2e8f0"/><circle cx="170" cy="60" r="35" fill="none" stroke="#4f46e5" stroke-width="14" stroke-dasharray="140 220"/><circle cx="170" cy="60" r="35" fill="none" stroke="#10b981" stroke-width="14" stroke-dasharray="60 220" stroke-dashoffset="-140"/><text x="18" y="45" fill="#0f172a" font-family="sans-serif" font-size="14" font-weight="700">Traffic Source</text><text x="18" y="70" fill="#4f46e5" font-size="11">● Direct: 65%</text><text x="18" y="88" fill="#10b981" font-size="11">● Social: 35%</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Chart / Donut Distribution',
      width: 260,
      height: 130,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 14,
        padding: [16, 20, 16, 20],
        cornerRadius: 12,
        fills: [{ type: 'solid', color: '#ffffff' }],
        strokes: [{ type: 'solid', color: '#e2e8f0', width: 1.5 }],
      },
      children: [
        {
          type: 'frame',
          name: 'Labels Column',
          width: 140,
          height: 90,
          props: { layout: 'vertical', itemSpacing: 6, sizingH: 'fill' },
          children: [
            { type: 'text', name: 'Title', characters: 'Traffic Sources', props: { fontSize: 14, fontWeight: 700, fills: [{ type: 'solid', color: '#0f172a' }] } },
            { type: 'text', name: 'Direct', characters: '● Direct: 65%', props: { fontSize: 12, fontWeight: 500, fills: [{ type: 'solid', color: '#4f46e5' }] } },
            { type: 'text', name: 'Social', characters: '● Organic: 35%', props: { fontSize: 12, fontWeight: 500, fills: [{ type: 'solid', color: '#10b981' }] } },
          ],
        },
        {
          type: 'ellipse',
          name: 'Donut Circle',
          width: 70,
          height: 70,
          props: {
            fills: [{ type: 'solid', color: '#e0e7ff' }],
            strokes: [{ type: 'solid', color: '#4f46e5', width: 8 }],
          },
        },
      ],
    },
  },

  // -----------------------------------------------------------------------
  // 5. KANBAN / COMPLEX
  // -----------------------------------------------------------------------
  {
    id: '@kibo/kanban-board',
    name: 'Multi-Column Kanban Board',
    sourceRegistry: '@kibo',
    category: 'Kanban/Complex',
    description: 'Agile project tracking board with columns (To Do, In Progress, Done) and draggable task cards.',
    tags: ['kanban', 'board', 'tasks', 'agile', 'project', 'kibo'],
    dependencies: ['@hello-pangea/dnd', 'lucide-react', 'clsx'],
    code: `export function KanbanBoard() {
  const columns = [
    { title: "To Do", count: 3, color: "bg-slate-400" },
    { title: "In Progress", count: 2, color: "bg-amber-500" },
    { title: "Completed", count: 5, color: "bg-emerald-500" }
  ];
  return (
    <div className="flex gap-4 p-4 bg-slate-50 dark:bg-neutral-900 rounded-2xl overflow-x-auto">
      {columns.map(col => (
        <div key={col.title} className="w-64 flex-shrink-0 bg-white dark:bg-neutral-800 rounded-xl p-3 border shadow-sm">
          <div className="flex items-center justify-between pb-3">
            <h4 className="text-sm font-semibold flex items-center gap-2">
              <span className={\`w-2 h-2 rounded-full \${col.color}\`} />
              {col.title}
            </h4>
            <span className="text-xs px-2 py-0.5 rounded-full bg-slate-100 font-mono">{col.count}</span>
          </div>
          <div className="space-y-2">
            <div className="p-3 bg-slate-50 dark:bg-neutral-700/50 rounded-lg text-xs font-medium border shadow-sm">Implement Market Tab</div>
            <div className="p-3 bg-slate-50 dark:bg-neutral-700/50 rounded-lg text-xs font-medium border shadow-sm">Audit Design System Tokens</div>
          </div>
        </div>
      ))}
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#7c3aed',
      width: 320,
      height: 180,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="320" height="180" viewBox="0 0 320 180"><rect width="320" height="180" rx="12" fill="#f8fafc" stroke="#e2e8f0"/><rect x="12" y="12" width="90" height="156" rx="8" fill="#ffffff" stroke="#e2e8f0"/><text x="20" y="32" fill="#0f172a" font-family="sans-serif" font-size="11" font-weight="700">TO DO (3)</text><rect x="18" y="44" width="78" height="38" rx="6" fill="#f1f5f9"/><text x="24" y="66" fill="#334155" font-size="9">Design Market</text><rect x="114" y="12" width="90" height="156" rx="8" fill="#ffffff" stroke="#e2e8f0"/><text x="122" y="32" fill="#f59e0b" font-family="sans-serif" font-size="11" font-weight="700">IN PROGRESS</text><rect x="120" y="44" width="78" height="38" rx="6" fill="#fef3c7"/><text x="126" y="66" fill="#b45309" font-size="9">AI Priority Tool</text><rect x="216" y="12" width="90" height="156" rx="8" fill="#ffffff" stroke="#e2e8f0"/><text x="224" y="32" fill="#10b981" font-family="sans-serif" font-size="11" font-weight="700">DONE (5)</text><rect x="222" y="44" width="78" height="38" rx="6" fill="#d1fae5"/><text x="228" y="66" fill="#047857" font-size="9">Clean Room CI</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Kanban / Sprint Board',
      width: 480,
      height: 260,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 16,
        padding: [16, 16, 16, 16],
        cornerRadius: 14,
        fills: [{ type: 'solid', color: '#f8fafc' }],
        strokes: [{ type: 'solid', color: '#e2e8f0', width: 1.5 }],
      },
      children: [
        {
          type: 'frame',
          name: 'Column / To Do',
          width: 140,
          height: 220,
          props: {
            layout: 'vertical',
            itemSpacing: 10,
            padding: [12, 12, 12, 12],
            cornerRadius: 10,
            fills: [{ type: 'solid', color: '#ffffff' }],
            strokes: [{ type: 'solid', color: '#e2e8f0', width: 1 }],
          },
          children: [
            { type: 'text', name: 'Col Header', characters: '📋 TO DO (3)', props: { fontSize: 12, fontWeight: 700, fills: [{ type: 'solid', color: '#475569' }] } },
            {
              type: 'frame',
              name: 'Card 1',
              width: 116,
              height: 52,
              props: { cornerRadius: 6, fills: [{ type: 'solid', color: '#f1f5f9' }], padding: [8, 8, 8, 8] },
              children: [{ type: 'text', name: 'Task', characters: 'Design Market UI', props: { fontSize: 11, fontWeight: 500 } }],
            },
          ],
        },
        {
          type: 'frame',
          name: 'Column / In Progress',
          width: 140,
          height: 220,
          props: {
            layout: 'vertical',
            itemSpacing: 10,
            padding: [12, 12, 12, 12],
            cornerRadius: 10,
            fills: [{ type: 'solid', color: '#ffffff' }],
            strokes: [{ type: 'solid', color: '#e2e8f0', width: 1 }],
          },
          children: [
            { type: 'text', name: 'Col Header', characters: '⚡ IN PROGRESS (2)', props: { fontSize: 12, fontWeight: 700, fills: [{ type: 'solid', color: '#f59e0b' }] } },
            {
              type: 'frame',
              name: 'Card 2',
              width: 116,
              height: 52,
              props: { cornerRadius: 6, fills: [{ type: 'solid', color: '#fef3c7' }], padding: [8, 8, 8, 8] },
              children: [{ type: 'text', name: 'Task', characters: 'AI Market Priority', props: { fontSize: 11, fontWeight: 500 } }],
            },
          ],
        },
        {
          type: 'frame',
          name: 'Column / Done',
          width: 140,
          height: 220,
          props: {
            layout: 'vertical',
            itemSpacing: 10,
            padding: [12, 12, 12, 12],
            cornerRadius: 10,
            fills: [{ type: 'solid', color: '#ffffff' }],
            strokes: [{ type: 'solid', color: '#e2e8f0', width: 1 }],
          },
          children: [
            { type: 'text', name: 'Col Header', characters: '✓ DONE (5)', props: { fontSize: 12, fontWeight: 700, fills: [{ type: 'solid', color: '#10b981' }] } },
            {
              type: 'frame',
              name: 'Card 3',
              width: 116,
              height: 52,
              props: { cornerRadius: 6, fills: [{ type: 'solid', color: '#d1fae5' }], padding: [8, 8, 8, 8] },
              children: [{ type: 'text', name: 'Task', characters: 'Shadcn Registries', props: { fontSize: 11, fontWeight: 500 } }],
            },
          ],
        },
      ],
    },
  },
  {
    id: '@bundui/data-table',
    name: 'Interactive Data Table',
    sourceRegistry: '@bundui',
    category: 'Kanban/Complex',
    description: 'High-density enterprise table layout with sorting headers, user avatars, and status badges.',
    tags: ['table', 'data', 'grid', 'bundui', 'complex', 'enterprise'],
    dependencies: ['@tanstack/react-table', 'lucide-react'],
    code: `export function DataTable() {
  const users = [
    { name: "Sarah Chen", role: "Product Lead", status: "Active" },
    { name: "Alex Kumar", role: "Core Engineer", status: "Active" },
    { name: "Elena Rostova", role: "Design Systems", status: "Review" }
  ];
  return (
    <div className="rounded-xl border bg-card overflow-hidden">
      <table className="w-full text-left text-xs">
        <thead className="bg-muted border-b text-muted-foreground uppercase">
          <tr>
            <th className="p-3">User</th>
            <th className="p-3">Role</th>
            <th className="p-3">Status</th>
          </tr>
        </thead>
        <tbody className="divide-y">
          {users.map(u => (
            <tr key={u.name} className="hover:bg-muted/50">
              <td className="p-3 font-medium">{u.name}</td>
              <td className="p-3 text-muted-foreground">{u.role}</td>
              <td className="p-3"><span className="px-2 py-0.5 rounded-full bg-emerald-100 text-emerald-700 text-[10px] font-semibold">{u.status}</span></td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#ea580c',
      width: 280,
      height: 140,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="280" height="140" viewBox="0 0 280 140"><rect width="280" height="140" rx="10" fill="#ffffff" stroke="#e2e8f0"/><rect y="0" width="280" height="34" rx="10" fill="#f8fafc"/><text x="14" y="22" fill="#64748b" font-family="sans-serif" font-size="11" font-weight="700">USER</text><text x="120" y="22" fill="#64748b" font-family="sans-serif" font-size="11" font-weight="700">ROLE</text><text x="210" y="22" fill="#64748b" font-family="sans-serif" font-size="11" font-weight="700">STATUS</text><line x1="0" y1="34" x2="280" y2="34" stroke="#e2e8f0"/><text x="14" y="60" fill="#0f172a" font-size="11" font-weight="600">Sarah Chen</text><text x="120" y="60" fill="#475569" font-size="11">Lead Design</text><rect x="210" y="48" width="48" height="18" rx="9" fill="#d1fae5"/><text x="234" y="61" fill="#065f46" font-size="9" font-weight="700" text-anchor="middle">ACTIVE</text><line x1="0" y1="78" x2="280" y2="78" stroke="#f1f5f9"/><text x="14" y="104" fill="#0f172a" font-size="11" font-weight="600">Alex Kumar</text><text x="120" y="104" fill="#475569" font-size="11">Staff Eng</text><rect x="210" y="92" width="48" height="18" rx="9" fill="#d1fae5"/><text x="234" y="105" fill="#065f46" font-size="9" font-weight="700" text-anchor="middle">ACTIVE</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Table / Data Grid',
      width: 380,
      height: 160,
      props: {
        layout: 'vertical',
        layoutMode: 'vertical',
        cornerRadius: 10,
        fills: [{ type: 'solid', color: '#ffffff' }],
        strokes: [{ type: 'solid', color: '#e2e8f0', width: 1.5 }],
      },
      children: [
        {
          type: 'frame',
          name: 'Table Header',
          width: 380,
          height: 36,
          props: { layout: 'horizontal', sizingH: 'fill', fills: [{ type: 'solid', color: '#f8fafc' }], padding: [10, 16, 10, 16] },
          children: [
            { type: 'text', name: 'H User', characters: 'NAME / USER', props: { fontSize: 11, fontWeight: 700, fills: [{ type: 'solid', color: '#64748b' }] } },
            { type: 'text', name: 'H Role', characters: 'ROLE', props: { fontSize: 11, fontWeight: 700, fills: [{ type: 'solid', color: '#64748b' }] } },
            { type: 'text', name: 'H Status', characters: 'STATUS', props: { fontSize: 11, fontWeight: 700, fills: [{ type: 'solid', color: '#64748b' }] } },
          ],
        },
        {
          type: 'frame',
          name: 'Row 1',
          width: 380,
          height: 48,
          props: { layout: 'horizontal', sizingH: 'fill', padding: [12, 16, 12, 16] },
          children: [
            { type: 'text', name: 'User 1', characters: 'Sarah Chen', props: { fontSize: 13, fontWeight: 600, fills: [{ type: 'solid', color: '#0f172a' }] } },
            { type: 'text', name: 'Role 1', characters: 'Lead Product Designer', props: { fontSize: 12, fills: [{ type: 'solid', color: '#475569' }] } },
          ],
        },
      ],
    },
  },

  // -----------------------------------------------------------------------
  // 6. ANIMATIONS
  // -----------------------------------------------------------------------
  {
    id: '@animate-ui/marquee-ticker',
    name: 'Infinite Marquee Ticker Ribbon',
    sourceRegistry: '@animate-ui',
    category: 'Animations',
    description: 'Continuous horizontal scrolling banner ribbon for partner logos, metrics, or announcements.',
    tags: ['marquee', 'animation', 'ticker', 'ribbon', 'animate-ui'],
    dependencies: ['framer-motion'],
    code: `export function MarqueeTicker({ items = ["⚡ 100% Free Ecosystem", "✦ Shadcn Verified", "🚀 Instant Canvas Placement", "💎 Pure Rust Engine"] }) {
  return (
    <div className="relative flex overflow-x-hidden bg-neutral-900 py-3 text-white border-y border-neutral-800">
      <div className="animate-marquee whitespace-nowrap flex gap-8 text-xs font-semibold uppercase tracking-wider">
        {items.map((item, idx) => (
          <span key={idx} className="flex items-center gap-2">{item}</span>
        ))}
      </div>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#059669',
      width: 280,
      height: 42,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="280" height="42" viewBox="0 0 280 42"><rect width="280" height="42" fill="#171717"/><text x="10" y="26" fill="#34d399" font-family="sans-serif" font-size="12" font-weight="700">⚡ 100% FREE</text><text x="110" y="26" fill="#ffffff" font-family="sans-serif" font-size="12" font-weight="600">✦ SHADCN VERIFIED</text><text x="245" y="26" fill="#60a5fa" font-family="sans-serif" font-size="12" font-weight="600">🚀 INSTANT</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Animation / Marquee Banner',
      width: 360,
      height: 44,
      props: {
        layout: 'horizontal',
        layoutMode: 'horizontal',
        itemSpacing: 18,
        padding: [12, 18, 12, 18],
        cornerRadius: 8,
        fills: [{ type: 'solid', color: '#171717' }],
      },
      children: [
        { type: 'text', name: 'Item 1', characters: '⚡ 100% Free Registries', props: { fontSize: 13, fontWeight: 700, fills: [{ type: 'solid', color: '#34d399' }] } },
        { type: 'text', name: 'Item 2', characters: '✦ Shadcn Ecosystem', props: { fontSize: 13, fontWeight: 600, fills: [{ type: 'solid', color: '#ffffff' }] } },
        { type: 'text', name: 'Item 3', characters: '🚀 Instant Canvas Drop', props: { fontSize: 13, fontWeight: 600, fills: [{ type: 'solid', color: '#60a5fa' }] } },
      ],
    },
  },
  {
    id: '@animate-ui/spotlight-card',
    name: 'Cursor Tracking Spotlight Card',
    sourceRegistry: '@animate-ui',
    category: 'Animations',
    description: 'Card with a dynamic radial gradient spotlight that smoothly follows mouse movement.',
    tags: ['spotlight', 'animation', 'cursor', 'gradient', 'animate-ui', 'modern'],
    dependencies: ['framer-motion'],
    code: `export function SpotlightCard({ title = "Spotlight Physics", description = "Dynamic radial illumination reacts to pointer movement across the surface." }) {
  return (
    <div className="relative rounded-2xl border border-neutral-800 bg-neutral-950 p-6 overflow-hidden">
      <div className="pointer-events-none absolute -inset-px opacity-50 bg-[radial-gradient(400px_circle_at_var(--mouse-x,100px)_var(--mouse-y,100px),rgba(99,102,241,0.15),transparent_80%)]" />
      <h4 className="text-base font-bold text-white">{title}</h4>
      <p className="mt-2 text-xs text-neutral-400">{description}</p>
    </div>
  );
}`,
    previewData: {
      badgeText: '100% Free',
      accentColor: '#059669',
      width: 240,
      height: 120,
      thumbnailSvg: `<svg xmlns="http://www.w3.org/2000/svg" width="240" height="120" viewBox="0 0 240 120"><rect width="240" height="120" rx="12" fill="#09090b" stroke="#27272a"/><circle cx="100" cy="50" r="60" fill="#4f46e5" opacity="0.25"/><text x="18" y="42" fill="#ffffff" font-family="sans-serif" font-size="14" font-weight="700">Spotlight Physics</text><text x="18" y="70" fill="#a1a1aa" font-family="sans-serif" font-size="11">Dynamic radial illumination reacts</text><text x="18" y="88" fill="#a1a1aa" font-family="sans-serif" font-size="11">to pointer movement in real time.</text></svg>`,
    },
    canvasTemplate: {
      type: 'frame',
      name: 'Animation / Spotlight Card',
      width: 260,
      height: 130,
      props: {
        layout: 'vertical',
        layoutMode: 'vertical',
        itemSpacing: 8,
        padding: [18, 20, 18, 20],
        cornerRadius: 12,
        fills: [{ type: 'solid', color: '#09090b' }],
        strokes: [{ type: 'solid', color: '#27272a', width: 1.5 }],
        effects: [{ type: 'drop-shadow', color: '#4f46e520', offset: [0, 0], radius: 16 }],
      },
      children: [
        { type: 'text', name: 'Title', characters: 'Spotlight Physics', props: { fontSize: 15, fontWeight: 700, fills: [{ type: 'solid', color: '#ffffff' }] } },
        { type: 'text', name: 'Body', characters: 'Dynamic radial illumination reacts to pointer movement across the surface.', props: { fontSize: 12, fontWeight: 400, fills: [{ type: 'solid', color: '#a1a1aa' }] } },
      ],
    },
  },
];

// =========================================================================
// Registry Service Engine with In-Memory + Browser Storage Caching
// =========================================================================

class MarketRegistryService {
  private cache: Map<string, MarketComponent> = new Map();
  private isInitialized = false;

  constructor() {
    this.init();
  }

  private init() {
    if (this.isInitialized) return;

    // 1. Seed prebuilt components
    for (const comp of PREBUILT_MARKET_COMPONENTS) {
      this.cache.set(comp.id, comp);
    }

    // 2. Hydrate from localStorage if running in browser
    if (typeof window !== 'undefined' && window.localStorage) {
      try {
        const stored = window.localStorage.getItem('sycnui_market_registry_v1');
        if (stored) {
          const parsed = JSON.parse(stored);
          if (Array.isArray(parsed)) {
            for (const item of parsed) {
              if (item?.id) {
                this.cache.set(item.id, item);
              }
            }
          }
        }
      } catch (err) {
        console.warn('[MarketRegistryService] Failed to hydrate cache from localStorage:', err);
      }
    }

    this.isInitialized = true;
  }

  private saveToStorage() {
    if (typeof window !== 'undefined' && window.localStorage) {
      try {
        const all = Array.from(this.cache.values());
        window.localStorage.setItem('sycnui_market_registry_v1', JSON.stringify(all));
      } catch (err) {
        console.warn('[MarketRegistryService] Failed to persist cache to localStorage:', err);
      }
    }
  }

  /**
   * Returns all indexed components
   */
  public getAll(): MarketComponent[] {
    this.init();
    return Array.from(this.cache.values());
  }

  /**
   * Retrieves a single component by its ID (e.g. '@shadcn/button')
   */
  public getById(id: string): MarketComponent | undefined {
    this.init();
    return this.cache.get(id);
  }

  /**
   * Search and filter components
   */
  public search(params: {
    query?: string;
    category?: MarketCategory;
    registry?: MarketRegistrySource | 'all';
  }): MarketComponent[] {
    this.init();
    const query = (params.query || '').trim().toLowerCase();
    const category = params.category || 'All';
    const registry = params.registry || 'all';

    return Array.from(this.cache.values()).filter((item) => {
      // 1. Category match
      if (category !== 'All' && item.category !== category) {
        return false;
      }

      // 2. Registry match
      if (registry !== 'all' && item.sourceRegistry !== registry) {
        return false;
      }

      // 3. Search query match
      if (query) {
        const inName = item.name.toLowerCase().includes(query);
        const inDesc = item.description.toLowerCase().includes(query);
        const inTags = item.tags.some((t) => t.toLowerCase().includes(query));
        const inId = item.id.toLowerCase().includes(query);
        return inName || inDesc || inTags || inId;
      }

      return true;
    });
  }

  /**
   * Attempts to fetch a component from public registry JSON endpoint
   * and normalizes it into MarketComponent schema.
   */
  public async fetchRemote(
    source: MarketRegistrySource,
    componentName: string
  ): Promise<MarketComponent | null> {
    const config = REGISTRY_CONFIGS[source];
    if (!config) return null;

    const url = config.baseUrl.replace('{name}', encodeURIComponent(componentName));
    try {
      const res = await fetch(url);
      if (res.ok) {
        const json = await res.json();
        const code = json.files?.[0]?.content || '';
        const id = `${source}/${componentName}`;

        const normalized: MarketComponent = {
          id,
          name: json.name || componentName,
          sourceRegistry: source,
          category: 'Cards', // default category
          description: json.description || `${componentName} from ${config.label}`,
          tags: [componentName, source.replace('@', ''), 'prebuilt'],
          code,
          dependencies: json.dependencies || [],
          previewData: {
            badgeText: '100% Free',
            accentColor: config.badgeColor,
            width: 240,
            height: 120,
          },
          canvasTemplate: {
            type: 'frame',
            name: `${config.label} / ${componentName}`,
            width: 240,
            height: 120,
            props: {
              layout: 'vertical',
              padding: [16, 16, 16, 16],
              cornerRadius: 10,
              fills: [{ type: 'solid', color: '#ffffff' }],
              strokes: [{ type: 'solid', color: '#e2e8f0', width: 1.5 }],
            },
            children: [
              {
                type: 'text',
                name: 'Title',
                characters: json.name || componentName,
                props: { fontSize: 15, fontWeight: 700 },
              },
            ],
          },
        };

        this.cache.set(id, normalized);
        this.saveToStorage();
        return normalized;
      }
    } catch (err) {
      console.warn(`[MarketRegistryService] Remote fetch failed for ${url}:`, err);
    }

    // Fallback to local cache if present
    return this.getById(`${source}/${componentName}`) || null;
  }
}

export const marketRegistry = new MarketRegistryService();
