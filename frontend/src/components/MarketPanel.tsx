/**
 * sycnUI Component Marketplace Explorer UI (MarketPanel)
 * 
 * Provides instant search, category filtering, source filtering,
 * interactive card previews, and direct "Add to Canvas" functionality.
 */

import React, { useState, useMemo, useRef, useEffect } from 'react';
import {
  Search,
  X,
  Sparkles,
  ShoppingBag,
  ExternalLink,
  Code,
  Plus,
  Check,
  Eye,
  Filter,
  Layers,
} from 'lucide-react';
import {
  marketRegistry,
  MarketCategory,
  MarketRegistrySource,
  MarketComponent,
  REGISTRY_CONFIGS,
} from '../lib/market-registry';
import { insertMarketComponentToCanvas } from '../utils/insertMarketComponentToCanvas';

export interface MarketPanelProps {
  isOpen: boolean;
  onClose?: () => void;
  onComponentInserted?: (nodeId: string, componentId: string) => void;
  className?: string;
}

const CATEGORIES: MarketCategory[] = [
  'All',
  'Buttons',
  'Inputs',
  'Cards',
  'Charts',
  'Kanban/Complex',
  'Animations',
];

const REGISTRIES: Array<{ label: string; value: MarketRegistrySource | 'all' }> = [
  { label: 'All Sources', value: 'all' },
  { label: 'shadcn/ui', value: '@shadcn' },
  { label: 'Origin UI', value: '@originui' },
  { label: 'Kibo UI', value: '@kibo' },
  { label: 'Cult UI', value: '@cult' },
  { label: 'Animate UI', value: '@animate-ui' },
  { label: 'Bund UI', value: '@bundui' },
  { label: 'Neobrutalism', value: '@neobrutalism' },
];

export const MarketPanel: React.FC<MarketPanelProps> = ({
  isOpen,
  onClose,
  onComponentInserted,
  className = '',
}) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedCategory, setSelectedCategory] = useState<MarketCategory>('All');
  const [selectedRegistry, setSelectedRegistry] = useState<MarketRegistrySource | 'all'>('all');
  const [previewModalComponent, setPreviewModalComponent] = useState<MarketComponent | null>(null);
  const [modalTab, setModalTab] = useState<'preview' | 'code'>('preview');
  const [copiedCode, setCopiedCode] = useState(false);
  const [insertingId, setInsertingId] = useState<string | null>(null);
  const [recentInsertedId, setRecentInsertedId] = useState<string | null>(null);

  const searchInputRef = useRef<HTMLInputElement>(null);

  // Focus search input on open
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => searchInputRef.current?.focus(), 60);
    }
  }, [isOpen]);

  // Filtered components
  const components = useMemo(() => {
    return marketRegistry.search({
      query: searchQuery,
      category: selectedCategory,
      registry: selectedRegistry,
    });
  }, [searchQuery, selectedCategory, selectedRegistry]);

  // Handle direct addition to canvas
  const handleAddToCanvas = async (component: MarketComponent) => {
    setInsertingId(component.id);
    try {
      const result = await insertMarketComponentToCanvas(component);
      if (result.success) {
        setRecentInsertedId(component.id);
        setTimeout(() => setRecentInsertedId(null), 2000);
        onComponentInserted?.(result.rootNodeId, component.id);
      }
    } catch (err) {
      console.error('[MarketPanel] Failed to insert component:', err);
    } finally {
      setInsertingId(null);
    }
  };

  // Copy code handler
  const handleCopyCode = (code: string) => {
    navigator.clipboard?.writeText(code);
    setCopiedCode(true);
    setTimeout(() => setCopiedCode(false), 2000);
  };

  if (!isOpen) return null;

  return (
    <>
      <aside
        className={`w-80 h-full flex flex-col bg-white dark:bg-[#181818] border-r border-gray-200 dark:border-[#2e2e2e] select-none text-sm z-20 ${className}`}
        aria-label="Component Marketplace"
        role="region"
      >
        {/* 1. Header */}
        <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-[#2e2e2e] bg-[#fafafa] dark:bg-[#151515]">
          <div className="flex items-center space-x-2">
            <div className="w-6 h-6 rounded-md bg-indigo-600/10 text-indigo-600 dark:bg-indigo-500/20 dark:text-indigo-400 flex items-center justify-center font-bold">
              <ShoppingBag className="w-3.5 h-3.5" />
            </div>
            <div>
              <div className="flex items-center gap-1.5">
                <h2 className="font-semibold text-gray-900 dark:text-gray-100 text-xs">Marketplace</h2>
                <span className="text-[10px] px-1.5 py-0.2 rounded font-semibold bg-emerald-500/10 text-emerald-600 dark:bg-emerald-500/20 dark:text-emerald-400 border border-emerald-500/20">
                  100% Free
                </span>
              </div>
              <p className="text-[10px] text-gray-500 dark:text-gray-400">Shadcn Community Ecosystem</p>
            </div>
          </div>
          {onClose && (
            <button
              onClick={onClose}
              className="p-1 rounded-md hover:bg-gray-200 dark:hover:bg-[#282828] text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 transition-colors"
              aria-label="Close market panel"
            >
              <X className="w-4 h-4" />
            </button>
          )}
        </div>

        {/* 2. Instant Search Bar */}
        <div className="p-3 pb-2">
          <div className="relative flex items-center">
            <Search className="w-3.5 h-3.5 absolute left-3 text-gray-400 pointer-events-none" />
            <input
              ref={searchInputRef}
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search buttons, cards, kanban, charts..."
              className="w-full pl-8 pr-8 py-1.5 bg-gray-50 dark:bg-[#222222] border border-gray-200 dark:border-[#383838] rounded-md text-xs text-gray-800 dark:text-gray-100 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
              aria-label="Search components"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery('')}
                className="absolute right-2.5 p-0.5 rounded-full hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-400"
                aria-label="Clear search"
              >
                <X className="w-3 h-3" />
              </button>
            )}
          </div>
        </div>

        {/* 3. Source Registry Filter Pills */}
        <div className="px-3 pb-2 flex items-center space-x-1 overflow-x-auto no-scrollbar">
          {REGISTRIES.map((reg) => {
            const isActive = selectedRegistry === reg.value;
            return (
              <button
                key={reg.value}
                onClick={() => setSelectedRegistry(reg.value)}
                className={`px-2 py-0.5 text-[11px] font-medium rounded-full whitespace-nowrap transition-colors ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-xs'
                    : 'bg-gray-100 dark:bg-[#252525] text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-[#303030]'
                }`}
              >
                {reg.label}
              </button>
            );
          })}
        </div>

        {/* 4. Category Pills */}
        <div className="px-3 pb-2 flex items-center space-x-1 overflow-x-auto no-scrollbar border-b border-gray-100 dark:border-[#282828]">
          {CATEGORIES.map((cat) => {
            const isActive = selectedCategory === cat;
            return (
              <button
                key={cat}
                onClick={() => setSelectedCategory(cat)}
                className={`px-2 py-1 text-xs font-medium rounded-md whitespace-nowrap transition-colors ${
                  isActive
                    ? 'bg-gray-900 dark:bg-gray-100 text-white dark:text-gray-900 font-semibold shadow-xs'
                    : 'text-gray-600 dark:text-gray-400 hover:text-gray-900 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-[#252525]'
                }`}
              >
                {cat}
              </button>
            );
          })}
        </div>

        {/* 5. Components Grid / Cards */}
        <div className="flex-1 overflow-y-auto px-3 py-2 space-y-3" role="list">
          {components.length === 0 ? (
            <div className="flex flex-col items-center justify-center h-48 text-center px-4 text-gray-400">
              <Layers className="w-8 h-8 mb-2 stroke-1 text-gray-400" />
              <p className="font-medium text-xs text-gray-600 dark:text-gray-300">No components found</p>
              <p className="text-[11px] mt-1 text-gray-400">Try searching for keywords like "button", "card", "kanban", or "chart"</p>
            </div>
          ) : (
            components.map((comp) => {
              const isInserting = insertingId === comp.id;
              const isRecent = recentInsertedId === comp.id;
              const regConfig = REGISTRY_CONFIGS[comp.sourceRegistry];

              return (
                <div
                  key={comp.id}
                  draggable
                  onDragStart={(e) => {
                    e.dataTransfer.setData('text/plain', JSON.stringify({ type: 'market-component', id: comp.id }));
                  }}
                  className="group relative flex flex-col rounded-xl border border-gray-200 dark:border-[#333333] bg-white dark:bg-[#1e1e1e] hover:border-indigo-400 dark:hover:border-indigo-500 hover:shadow-md transition-all duration-150 overflow-hidden"
                >
                  {/* Visual Preview / Thumbnail Area */}
                  <div
                    onClick={() => setPreviewModalComponent(comp)}
                    className="w-full h-28 bg-[#f8fafc] dark:bg-[#141414] border-b border-gray-100 dark:border-[#2a2a2a] flex items-center justify-center p-2 cursor-pointer relative group/preview overflow-hidden"
                  >
                    {comp.previewData.thumbnailSvg ? (
                      <div
                        className="max-w-full max-h-full flex items-center justify-center select-none pointer-events-none transform group-hover/preview:scale-105 transition-transform"
                        dangerouslySetInnerHTML={{ __html: comp.previewData.thumbnailSvg }}
                      />
                    ) : (
                      <div className="text-xs text-gray-400 font-mono flex items-center gap-1">
                        <Code className="w-4 h-4" />
                        <span>JSX Preview</span>
                      </div>
                    )}

                    {/* Hover Overlay with Preview Icon */}
                    <div className="absolute inset-0 bg-black/40 opacity-0 group-hover/preview:opacity-100 flex items-center justify-center transition-opacity text-white text-xs font-medium gap-1 backdrop-blur-xs">
                      <Eye className="w-3.5 h-3.5" />
                      <span>Inspect Details</span>
                    </div>

                    {/* Source Registry Tag */}
                    <span
                      style={{ borderColor: `${regConfig?.badgeColor || '#6366f1'}40` }}
                      className="absolute top-2 left-2 text-[9px] font-bold px-1.5 py-0.5 rounded bg-white/90 dark:bg-black/90 text-gray-800 dark:text-gray-200 border shadow-xs"
                    >
                      {regConfig?.label || comp.sourceRegistry}
                    </span>
                  </div>

                  {/* Component Info & Actions */}
                  <div className="p-3">
                    <div className="flex items-start justify-between gap-1">
                      <h3
                        onClick={() => setPreviewModalComponent(comp)}
                        className="font-semibold text-xs text-gray-900 dark:text-gray-100 line-clamp-1 cursor-pointer hover:text-indigo-600 dark:hover:text-indigo-400"
                      >
                        {comp.name}
                      </h3>
                      <span className="text-[10px] text-gray-400 font-medium whitespace-nowrap">
                        {comp.category}
                      </span>
                    </div>
                    <p className="text-[11px] text-gray-500 dark:text-gray-400 line-clamp-2 mt-1 leading-snug">
                      {comp.description}
                    </p>

                    {/* Action Buttons: Preview & Add to Canvas */}
                    <div className="flex items-center gap-1.5 mt-3 pt-2 border-t border-gray-100 dark:border-[#282828]">
                      <button
                        onClick={() => setPreviewModalComponent(comp)}
                        className="flex-1 py-1 px-2 rounded-md bg-gray-100 dark:bg-[#282828] text-gray-700 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-[#323232] text-xs font-medium flex items-center justify-center gap-1 transition-colors"
                      >
                        <Eye className="w-3 h-3" />
                        <span>Preview</span>
                      </button>

                      <button
                        onClick={() => handleAddToCanvas(comp)}
                        disabled={isInserting}
                        className={`flex-1 py-1 px-2 rounded-md text-xs font-semibold flex items-center justify-center gap-1 transition-all ${
                          isRecent
                            ? 'bg-emerald-600 text-white'
                            : 'bg-indigo-600 text-white hover:bg-indigo-700 active:scale-95 shadow-xs'
                        }`}
                      >
                        {isRecent ? (
                          <>
                            <Check className="w-3 h-3" />
                            <span>Added!</span>
                          </>
                        ) : (
                          <>
                            <Plus className="w-3 h-3" />
                            <span>Add to Canvas</span>
                          </>
                        )}
                      </button>
                    </div>
                  </div>
                </div>
              );
            })
          )}
        </div>
      </aside>

      {/* 6. Detail / Code Preview Modal */}
      {previewModalComponent && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-xs p-4 animate-in fade-in duration-150"
          onClick={() => setPreviewModalComponent(null)}
        >
          <div
            className="w-full max-w-2xl bg-white dark:bg-[#1a1a1a] rounded-2xl shadow-2xl border border-gray-200 dark:border-[#333333] overflow-hidden flex flex-col max-h-[85vh]"
            onClick={(e) => e.stopPropagation()}
          >
            {/* Modal Header */}
            <div className="flex items-center justify-between px-6 py-4 border-b border-gray-200 dark:border-[#2e2e2e]">
              <div>
                <div className="flex items-center gap-2">
                  <h3 className="font-bold text-base text-gray-900 dark:text-gray-100">
                    {previewModalComponent.name}
                  </h3>
                  <span className="text-[10px] font-semibold px-2 py-0.5 rounded-full bg-indigo-50 dark:bg-indigo-950/60 text-indigo-600 dark:text-indigo-400 border border-indigo-200 dark:border-indigo-800">
                    {previewModalComponent.sourceRegistry} • 100% Free
                  </span>
                </div>
                <p className="text-xs text-gray-500 dark:text-gray-400 mt-0.5">
                  {previewModalComponent.description}
                </p>
              </div>
              <button
                onClick={() => setPreviewModalComponent(null)}
                className="p-1 rounded-md hover:bg-gray-100 dark:hover:bg-[#282828] text-gray-400 hover:text-gray-600"
              >
                <X className="w-5 h-5" />
              </button>
            </div>

            {/* Modal Tabs (Visual vs Code) */}
            <div className="flex items-center justify-between px-6 py-2 border-b border-gray-100 dark:border-[#282828] bg-gray-50 dark:bg-[#161616]">
              <div className="flex items-center space-x-2">
                <button
                  onClick={() => setModalTab('preview')}
                  className={`px-3 py-1 text-xs font-semibold rounded-md transition-colors ${
                    modalTab === 'preview'
                      ? 'bg-white dark:bg-[#262626] text-gray-900 dark:text-white shadow-xs'
                      : 'text-gray-500 hover:text-gray-800 dark:hover:text-gray-200'
                  }`}
                >
                  Visual Preview
                </button>
                <button
                  onClick={() => setModalTab('code')}
                  className={`px-3 py-1 text-xs font-semibold rounded-md transition-colors ${
                    modalTab === 'code'
                      ? 'bg-white dark:bg-[#262626] text-gray-900 dark:text-white shadow-xs'
                      : 'text-gray-500 hover:text-gray-800 dark:hover:text-gray-200'
                  }`}
                >
                  React JSX Code
                </button>
              </div>

              {modalTab === 'code' && (
                <button
                  onClick={() => handleCopyCode(previewModalComponent.code)}
                  className="px-2.5 py-1 text-xs font-medium rounded bg-indigo-600 text-white hover:bg-indigo-700 flex items-center gap-1 transition-colors"
                >
                  {copiedCode ? <Check className="w-3 h-3" /> : <Code className="w-3 h-3" />}
                  <span>{copiedCode ? 'Copied!' : 'Copy Code'}</span>
                </button>
              )}
            </div>

            {/* Modal Body */}
            <div className="flex-1 overflow-y-auto p-6">
              {modalTab === 'preview' ? (
                <div className="flex flex-col items-center justify-center p-8 bg-gray-50 dark:bg-[#121212] rounded-xl border border-gray-200 dark:border-[#2e2e2e] min-h-[220px]">
                  {previewModalComponent.previewData.thumbnailSvg ? (
                    <div
                      className="transform scale-125"
                      dangerouslySetInnerHTML={{ __html: previewModalComponent.previewData.thumbnailSvg }}
                    />
                  ) : (
                    <p className="text-xs text-gray-400 font-mono">Visual preview rendering ready</p>
                  )}
                </div>
              ) : (
                <pre className="p-4 bg-gray-900 text-gray-100 rounded-xl text-xs font-mono overflow-x-auto leading-relaxed border border-gray-800">
                  <code>{previewModalComponent.code}</code>
                </pre>
              )}

              {/* Dependencies Section */}
              {previewModalComponent.dependencies.length > 0 && (
                <div className="mt-4">
                  <h4 className="text-xs font-semibold text-gray-700 dark:text-gray-300 mb-2">
                    Package Dependencies:
                  </h4>
                  <div className="flex flex-wrap gap-1.5">
                    {previewModalComponent.dependencies.map((dep) => (
                      <span
                        key={dep}
                        className="px-2 py-0.5 rounded bg-gray-100 dark:bg-[#252525] text-gray-700 dark:text-gray-300 text-[11px] font-mono border border-gray-200 dark:border-[#383838]"
                      >
                        {dep}
                      </span>
                    ))}
                  </div>
                </div>
              )}
            </div>

            {/* Modal Footer */}
            <div className="flex items-center justify-end gap-3 px-6 py-3 border-t border-gray-200 dark:border-[#2e2e2e] bg-gray-50 dark:bg-[#151515]">
              <button
                onClick={() => setPreviewModalComponent(null)}
                className="px-4 py-2 rounded-lg text-xs font-medium text-gray-600 dark:text-gray-300 hover:bg-gray-100 dark:hover:bg-[#252525]"
              >
                Close
              </button>
              <button
                onClick={() => {
                  handleAddToCanvas(previewModalComponent);
                  setPreviewModalComponent(null);
                }}
                className="px-5 py-2 rounded-lg text-xs font-bold bg-indigo-600 hover:bg-indigo-700 text-white shadow-sm flex items-center gap-1.5"
              >
                <Plus className="w-4 h-4" />
                <span>Instantiate on Canvas</span>
              </button>
            </div>
          </div>
        </div>
      )}
    </>
  );
};
