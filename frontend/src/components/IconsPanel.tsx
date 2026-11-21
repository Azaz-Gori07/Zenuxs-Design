/**
 * sycnUI Virtualized Icons Panel Component
 * High-performance 5-column grid powered by Font Awesome Free (v6) via Iconify API.
 */

import React, { useState, useEffect, useRef, useMemo, useCallback } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { Icon as IconifyIcon } from '@iconify/react';
import { Search, X, Loader2, Sparkles, AlertCircle } from 'lucide-react';
import { insertIconToCanvas } from '../utils/insertIconToCanvas';
import { fetchRawSvg, CURATED_FA6_ICONS } from '../ai/aiIconTools';

export interface IconsPanelProps {
  isOpen: boolean;
  onClose?: () => void;
  onIconInserted?: (nodeId: string, iconId: string) => void;
  className?: string;
}

const COLUMNS = 5;
const ROW_HEIGHT = 52;

// Font Awesome Free (v6) collections
export const POPULAR_COLLECTIONS = [
  { label: 'All FA', prefix: '' }, // Defaults to searching all
  { label: 'Solid', prefix: 'fa6-solid' },
  { label: 'Regular', prefix: 'fa6-regular' },
  { label: 'Brands', prefix: 'fa6-brands' },
];

export const IconsPanel: React.FC<IconsPanelProps> = ({
  isOpen,
  onClose,
  onIconInserted,
  className = '',
}) => {
  const [searchQuery, setSearchQuery] = useState('');
  const [selectedPrefix, setSelectedPrefix] = useState<string>('');
  const [icons, setIcons] = useState<string[]>(CURATED_FA6_ICONS);
  const [isLoading, setIsLoading] = useState(false);
  const [page, setPage] = useState(1);
  const [hasMore, setHasMore] = useState(true);
  const [insertingId, setInsertingId] = useState<string | null>(null);

  const parentRef = useRef<HTMLDivElement>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  // Focus search input when panel opens
  useEffect(() => {
    if (isOpen) {
      setTimeout(() => searchInputRef.current?.focus(), 50);
    }
  }, [isOpen]);

  // Fetch Font Awesome Free icons from Iconify API based on query & prefix
  const fetchIcons = useCallback(
    async (query: string, prefix: string, pageNum: number, append = false) => {
      setIsLoading(true);
      const trimmedQuery = query.trim().toLowerCase();

      try {
        let endpoint = '';
        const limit = 100;
        const start = (pageNum - 1) * limit;

        if (trimmedQuery) {
          if (prefix) {
            endpoint = `https://api.iconify.design/search?query=${encodeURIComponent(
              trimmedQuery
            )}&prefix=${prefix}&limit=${limit}&start=${start}`;
          } else {
            endpoint = `https://api.iconify.design/search?query=${encodeURIComponent(
              trimmedQuery
            )}&prefixes=fa6-solid,fa6-regular,fa6-brands&limit=${limit}&start=${start}`;
          }
        } else {
          // If no search query, list icons from the chosen collection
          const activePrefix = prefix || 'fa6-solid';
          endpoint = `https://api.iconify.design/collection?prefix=${activePrefix}&limit=${limit}`;
        }

        const res = await fetch(endpoint);
        if (res.ok) {
          const data = await res.json();
          let fetchedIcons: string[] = [];

          if (Array.isArray(data.icons)) {
            fetchedIcons = data.icons;
          } else if (data.uncategorized && Array.isArray(data.uncategorized)) {
            const p = data.prefix || prefix || 'fa6-solid';
            fetchedIcons = data.uncategorized.map((name: string) => `${p}:${name}`);
          }

          setIcons((prev) => (append ? [...prev, ...fetchedIcons] : fetchedIcons));
          setHasMore(fetchedIcons.length >= limit);
        } else {
          throw new Error('API response not ok');
        }
      } catch (err) {
        console.warn('[IconsPanel] Iconify API fetch failed, using fallback:', err);
        if (!append) {
          const filtered = CURATED_FA6_ICONS.filter((id) => {
            const matchesQuery = !trimmedQuery || id.toLowerCase().includes(trimmedQuery);
            const matchesPrefix = !prefix || id.startsWith(`${prefix}:`);
            return matchesQuery && matchesPrefix;
          });
          setIcons(filtered);
          setHasMore(false);
        }
      } finally {
        setIsLoading(false);
      }
    },
    []
  );

  // Trigger search with debounce
  useEffect(() => {
    const timer = setTimeout(() => {
      setPage(1);
      fetchIcons(searchQuery, selectedPrefix, 1, false);
    }, 250);
    return () => clearTimeout(timer);
  }, [searchQuery, selectedPrefix, fetchIcons]);

  // Handle infinite scrolling
  const handleScroll = useCallback(() => {
    if (!parentRef.current || isLoading || !hasMore) return;
    const { scrollTop, scrollHeight, clientHeight } = parentRef.current;
    if (scrollTop + clientHeight >= scrollHeight - 120) {
      const nextPage = page + 1;
      setPage(nextPage);
      fetchIcons(searchQuery, selectedPrefix, nextPage, true);
    }
  }, [isLoading, hasMore, page, searchQuery, selectedPrefix, fetchIcons]);

  // Handle icon insertion
  const handleInsert = async (iconId: string) => {
    setInsertingId(iconId);
    try {
      const rawSvg = await fetchRawSvg(iconId);
      const [name] = iconId.includes(':') ? iconId.split(':').reverse() : [iconId];
      const result = await insertIconToCanvas(rawSvg, undefined, undefined, {
        name: `Icon / ${name}`,
      });
      if (result.success && onIconInserted) {
        onIconInserted(result.nodeId, iconId);
      }
    } catch (err) {
      console.error('[IconsPanel] Failed to insert icon:', err);
    } finally {
      setInsertingId(null);
    }
  };

  // Virtualizer row computation
  const rowCount = useMemo(() => Math.ceil(icons.length / COLUMNS), [icons.length]);

  const rowVirtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => ROW_HEIGHT,
    overscan: 4,
  });

  if (!isOpen) return null;

  return (
    <aside
      className={`w-72 h-full flex flex-col bg-white dark:bg-[#1e1e1e] border-r border-gray-200 dark:border-[#333333] select-none text-sm z-20 ${className}`}
      aria-label="Font Awesome Icons Library"
      role="region"
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-gray-200 dark:border-[#333333]">
        <div className="flex items-center space-x-2">
          <Sparkles className="w-4 h-4 text-indigo-500" />
          <h2 className="font-semibold text-gray-800 dark:text-gray-100">Font Awesome Free</h2>
          <span className="text-xs px-1.5 py-0.5 rounded-full bg-gray-100 dark:bg-gray-800 text-gray-500 font-mono">
            {icons.length > 0 ? `${icons.length}+` : '0'}
          </span>
        </div>
        {onClose && (
          <button
            onClick={onClose}
            className="p-1 rounded hover:bg-gray-100 dark:hover:bg-gray-800 text-gray-400 hover:text-gray-700 dark:hover:text-gray-200 focus:outline-none focus:ring-2 focus:ring-indigo-500"
            aria-label="Close icons panel"
          >
            <X className="w-4 h-4" />
          </button>
        )}
      </div>

      {/* Search Input */}
      <div className="p-3 pb-2">
        <div className="relative flex items-center">
          <Search className="w-4 h-4 absolute left-3 text-gray-400 pointer-events-none" />
          <input
            ref={searchInputRef}
            type="text"
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            placeholder="Search Font Awesome Free (v6)..."
            className="w-full pl-9 pr-8 py-1.5 bg-gray-50 dark:bg-[#282828] border border-gray-200 dark:border-[#404040] rounded-md text-xs text-gray-800 dark:text-gray-100 placeholder-gray-400 focus:outline-none focus:ring-2 focus:ring-indigo-500 focus:border-transparent transition-all"
            aria-label="Search Font Awesome icons"
          />
          {searchQuery && (
            <button
              onClick={() => setSearchQuery('')}
              className="absolute right-2.5 p-0.5 rounded-full hover:bg-gray-200 dark:hover:bg-gray-700 text-gray-400"
              aria-label="Clear search"
            >
              <X className="w-3.5 h-3.5" />
            </button>
          )}
        </div>
      </div>

      {/* Font Awesome Category Filter Chips */}
      <div className="px-3 pb-2 flex items-center space-x-1.5 overflow-x-auto no-scrollbar">
        {POPULAR_COLLECTIONS.map((col) => {
          const isActive = selectedPrefix === col.prefix;
          return (
            <button
              key={col.label}
              onClick={() => setSelectedPrefix(col.prefix)}
              className={`px-2.5 py-1 text-xs font-medium rounded-full whitespace-nowrap transition-colors ${
                isActive
                  ? 'bg-indigo-600 text-white shadow-sm'
                  : 'bg-gray-100 dark:bg-[#282828] text-gray-600 dark:text-gray-300 hover:bg-gray-200 dark:hover:bg-gray-700'
              }`}
            >
              {col.label}
            </button>
          );
        })}
      </div>

      {/* Virtualized Grid */}
      <div
        ref={parentRef}
        onScroll={handleScroll}
        className="flex-1 overflow-y-auto px-3 py-1 outline-none"
        tabIndex={0}
        role="grid"
        aria-label="Font Awesome Icons Grid"
      >
        {icons.length === 0 && !isLoading ? (
          <div className="flex flex-col items-center justify-center h-48 text-center px-4 text-gray-400">
            <AlertCircle className="w-8 h-8 mb-2 stroke-1 text-gray-400" />
            <p className="font-medium text-xs text-gray-600 dark:text-gray-300">No Font Awesome icons found</p>
            <p className="text-[11px] mt-1 text-gray-400">Try searching for keywords like "house", "user", "heart", or "bell"</p>
          </div>
        ) : (
          <div
            style={{
              height: `${rowVirtualizer.getTotalSize()}px`,
              width: '100%',
              position: 'relative',
            }}
          >
            {rowVirtualizer.getVirtualItems().map((virtualRow) => {
              const startIndex = virtualRow.index * COLUMNS;
              const rowIcons = icons.slice(startIndex, startIndex + COLUMNS);

              return (
                <div
                  key={virtualRow.key}
                  role="row"
                  style={{
                    position: 'absolute',
                    top: 0,
                    left: 0,
                    width: '100%',
                    height: `${virtualRow.size}px`,
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                  className="grid grid-cols-5 gap-2 py-1"
                >
                  {rowIcons.map((iconId) => {
                    const isInserting = insertingId === iconId;
                    const tooltipName = iconId.includes(':') ? iconId.split(':')[1] : iconId;

                    return (
                      <div
                        key={iconId}
                        role="gridcell"
                        tabIndex={0}
                        onClick={() => handleInsert(iconId)}
                        onKeyDown={(e) => {
                          if (e.key === 'Enter' || e.key === ' ') {
                            e.preventDefault();
                            handleInsert(iconId);
                          }
                        }}
                        title={`${tooltipName} (${iconId})`}
                        aria-label={`Insert ${tooltipName} icon`}
                        className={`group relative flex items-center justify-center h-10 w-10 rounded-md border border-transparent hover:border-indigo-400 dark:hover:border-indigo-500 hover:bg-indigo-50/50 dark:hover:bg-indigo-950/30 cursor-pointer transition-all duration-150 focus:outline-none focus:ring-2 focus:ring-indigo-500 ${
                          isInserting ? 'opacity-50 pointer-events-none' : ''
                        }`}
                      >
                        {isInserting ? (
                          <Loader2 className="w-5 h-5 animate-spin text-indigo-500" />
                        ) : (
                          <IconifyIcon
                            icon={iconId}
                            className="w-5 h-5 text-gray-700 dark:text-gray-200 group-hover:scale-110 group-hover:text-indigo-600 dark:group-hover:text-indigo-400 transition-transform"
                          />
                        )}
                      </div>
                    );
                  })}
                </div>
              );
            })}
          </div>
        )}

        {/* Loading Spinner / Pagination Indicator */}
        {isLoading && (
          <div className="flex items-center justify-center py-4 space-x-2 text-xs text-gray-400">
            <Loader2 className="w-4 h-4 animate-spin text-indigo-500" />
            <span>Loading Font Awesome icons...</span>
          </div>
        )}
      </div>
    </aside>
  );
};
