/**
 * sycnUI Primary Left Navigation Rail (SycnLeftNav)
 * Figma-like navigation rail with Home, File, Assets, Variables, and Icons (below Variables).
 */

import React, { useState } from 'react';
import { Home, Layers, Component, Sliders, Shapes } from 'lucide-react';
import { IconsPanel } from './IconsPanel';

export type LeftNavTab = 'home' | 'file' | 'assets' | 'variables' | 'icons';

export interface SycnLeftNavProps {
  initialTab?: LeftNavTab;
  onTabChange?: (tab: LeftNavTab) => void;
  onIconInserted?: (nodeId: string, iconId: string) => void;
  className?: string;
}

export const SycnLeftNav: React.FC<SycnLeftNavProps> = ({
  initialTab = 'file',
  onTabChange,
  onIconInserted,
  className = '',
}) => {
  const [activeTab, setActiveTab] = useState<LeftNavTab>(initialTab);
  const [isPanelOpen, setIsPanelOpen] = useState(true);

  const handleTabClick = (tab: LeftNavTab) => {
    if (activeTab === tab) {
      // Toggle secondary panel open/closed if clicked again
      setIsPanelOpen(!isPanelOpen);
    } else {
      setActiveTab(tab);
      setIsPanelOpen(true);
      onTabChange?.(tab);
    }
  };

  const navItems = [
    { id: 'home' as LeftNavTab, label: 'Home', icon: Home },
    { id: 'file' as LeftNavTab, label: 'File', icon: Layers },
    { id: 'assets' as LeftNavTab, label: 'Assets', icon: Component },
    { id: 'variables' as LeftNavTab, label: 'Variables', icon: Sliders },
    // "Icons" tab placed exactly below "Variables" using Shapes icon
    { id: 'icons' as LeftNavTab, label: 'Icons', icon: Shapes },
  ];

  return (
    <div className={`flex h-screen overflow-hidden bg-white dark:bg-[#181818] border-r border-gray-200 dark:border-[#2e2e2e] ${className}`}>
      {/* 1. Primary Left Navigation Rail (48px wide) */}
      <nav
        className="w-12 h-full flex flex-col items-center py-3 bg-[#fbfbfb] dark:bg-[#141414] border-r border-gray-200 dark:border-[#282828] select-none z-30"
        aria-label="Primary Navigation"
      >
        {/* App Logo */}
        <div className="w-8 h-8 mb-4 rounded-lg flex items-center justify-center bg-indigo-600/10 text-indigo-600 font-bold text-sm">
          S
        </div>

        {/* Navigation Tabs */}
        <div className="flex-1 flex flex-col items-center space-y-2 w-full px-1.5">
          {navItems.map((item) => {
            const Icon = item.icon;
            const isActive = activeTab === item.id && isPanelOpen;

            return (
              <button
                key={item.id}
                onClick={() => handleTabClick(item.id)}
                title={item.label}
                aria-label={item.label}
                aria-pressed={isActive}
                className={`w-9 h-9 flex flex-col items-center justify-center rounded-lg transition-all duration-150 relative group ${
                  isActive
                    ? 'bg-indigo-600 text-white shadow-sm'
                    : 'text-gray-500 hover:text-gray-800 dark:text-gray-400 dark:hover:text-gray-200 hover:bg-gray-100 dark:hover:bg-[#202020]'
                }`}
              >
                <Icon className="w-4 h-4 stroke-[1.75]" />
                {/* Active indicator bar */}
                {isActive && (
                  <span className="absolute -left-1.5 top-2 bottom-2 w-0.5 bg-indigo-600 rounded-r" />
                )}
                {/* Hover Tooltip */}
                <span className="absolute left-12 px-2 py-1 bg-gray-900 text-white text-[11px] font-medium rounded shadow-md opacity-0 group-hover:opacity-100 pointer-events-none transition-opacity whitespace-nowrap z-50">
                  {item.label}
                </span>
              </button>
            );
          })}
        </div>
      </nav>

      {/* 2. Secondary Adjacent Panel */}
      {isPanelOpen && (
        <div className="h-full flex-shrink-0 animate-in slide-in-from-left-2 duration-150">
          {activeTab === 'icons' && (
            <IconsPanel
              isOpen={isPanelOpen}
              onClose={() => setIsPanelOpen(false)}
              onIconInserted={onIconInserted}
            />
          )}

          {activeTab === 'file' && (
            <div className="w-64 h-full p-4 border-r border-gray-200 dark:border-[#333333] text-xs text-gray-500">
              <h3 className="font-semibold text-gray-800 dark:text-gray-200 mb-2">Layers</h3>
              <p>Document layer hierarchy</p>
            </div>
          )}

          {activeTab === 'assets' && (
            <div className="w-64 h-full p-4 border-r border-gray-200 dark:border-[#333333] text-xs text-gray-500">
              <h3 className="font-semibold text-gray-800 dark:text-gray-200 mb-2">Assets</h3>
              <p>Components & shared library assets</p>
            </div>
          )}

          {activeTab === 'variables' && (
            <div className="w-64 h-full p-4 border-r border-gray-200 dark:border-[#333333] text-xs text-gray-500">
              <h3 className="font-semibold text-gray-800 dark:text-gray-200 mb-2">Variables</h3>
              <p>Design tokens, colors, numbers, and strings</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
};
