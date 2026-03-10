/**
 * Tree Controls Component
 *
 * Provides sorting, filtering, and expansion controls for ontology tree
 */

import React from 'react';
import {
  ArrowUpAZ,
  ArrowDownAZ,
  ChevronDown,
  ChevronRight,
  Minimize2,
  Maximize2,
  Filter,
  X,
} from 'lucide-react';
import { Button } from '../ui/button';

export type SortOption = 'alpha-asc' | 'alpha-desc' | 'depth' | 'properties';
export type FilterOption = 'all' | 'classes' | 'properties' | 'deprecated';

interface TreeControlsProps {
  sortBy: SortOption;
  onSortChange: (sort: SortOption) => void;
  filterBy: FilterOption;
  onFilterChange: (filter: FilterOption) => void;
  allExpanded: boolean;
  onExpandAll: () => void;
  onCollapseAll: () => void;
  totalClasses: number;
  totalProperties: number;
  visibleClasses: number;
  visibleProperties: number;
}

export const TreeControls: React.FC<TreeControlsProps> = ({
  sortBy,
  onSortChange,
  filterBy,
  onFilterChange,
  allExpanded,
  onExpandAll,
  onCollapseAll,
  totalClasses,
  totalProperties,
  visibleClasses,
  visibleProperties,
}) => {
  const [showFilterMenu, setShowFilterMenu] = React.useState(false);

  return (
    <div className="flex items-center justify-between p-3 bg-muted/60 border border-border rounded-lg shadow-sm">
      {/* Left: Stats */}
      <div className="flex items-center gap-4 text-sm">
        <span className="flex items-center gap-1.5">
          <span className="text-muted-foreground">Classes:</span>
          <strong className="text-foreground font-semibold">{visibleClasses}</strong>
          {visibleClasses !== totalClasses && <span className="text-muted-foreground text-xs">/{totalClasses}</span>}
        </span>
        <span className="flex items-center gap-1.5">
          <span className="text-muted-foreground">Properties:</span>
          <strong className="text-foreground font-semibold">{visibleProperties}</strong>
          {visibleProperties !== totalProperties && <span className="text-muted-foreground text-xs">/{totalProperties}</span>}
        </span>
      </div>

      {/* Right: Controls */}
      <div className="flex items-center gap-1">
        {/* Sort Dropdown */}
        <div className="relative">
          <select
            value={sortBy}
            onChange={(e) => onSortChange(e.target.value as SortOption)}
            className="h-8 px-2 pr-6 text-xs border border-border rounded bg-background hover:bg-muted/50 cursor-pointer focus:outline-none focus:ring-2 focus:ring-accent"
          >
            <option value="alpha-asc">A → Z</option>
            <option value="alpha-desc">Z → A</option>
            <option value="depth">By Depth</option>
            <option value="properties">By Properties</option>
          </select>
        </div>

        {/* Filter Button */}
        <div className="relative">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowFilterMenu(!showFilterMenu)}
            className={`h-8 px-2 ${filterBy !== 'all' ? 'bg-accent' : ''}`}
          >
            <Filter className="h-4 w-4 mr-1" />
            {filterBy !== 'all' && <span className="text-xs">Filtered</span>}
          </Button>

          {showFilterMenu && (
            <>
              {/* Backdrop */}
              <div
                className="fixed inset-0 z-10"
                onClick={() => setShowFilterMenu(false)}
              />

              {/* Menu */}
              <div className="absolute right-0 top-full mt-1 w-40 bg-card border border-border rounded-lg shadow-lg z-20 py-1">
                <button
                  onClick={() => {
                    onFilterChange('all');
                    setShowFilterMenu(false);
                  }}
                  className={`w-full px-3 py-2 text-left text-sm hover:bg-muted/50 ${
                    filterBy === 'all' ? 'bg-accent' : ''
                  }`}
                >
                  Show All
                </button>
                <button
                  onClick={() => {
                    onFilterChange('classes');
                    setShowFilterMenu(false);
                  }}
                  className={`w-full px-3 py-2 text-left text-sm hover:bg-muted/50 ${
                    filterBy === 'classes' ? 'bg-accent' : ''
                  }`}
                >
                  Classes Only
                </button>
                <button
                  onClick={() => {
                    onFilterChange('properties');
                    setShowFilterMenu(false);
                  }}
                  className={`w-full px-3 py-2 text-left text-sm hover:bg-muted/50 ${
                    filterBy === 'properties' ? 'bg-accent' : ''
                  }`}
                >
                  Properties Only
                </button>
                <button
                  onClick={() => {
                    onFilterChange('deprecated');
                    setShowFilterMenu(false);
                  }}
                  className={`w-full px-3 py-2 text-left text-sm hover:bg-muted/50 ${
                    filterBy === 'deprecated' ? 'bg-accent' : ''
                  }`}
                >
                  Hide Deprecated
                </button>
              </div>
            </>
          )}
        </div>

        {/* Expand/Collapse */}
        <Button
          variant="ghost"
          size="sm"
          onClick={allExpanded ? onCollapseAll : onExpandAll}
          title={allExpanded ? 'Collapse All' : 'Expand All'}
          className="h-8 px-2"
        >
          {allExpanded ? (
            <>
              <Minimize2 className="h-4 w-4 mr-1" />
              <span className="text-xs">Collapse</span>
            </>
          ) : (
            <>
              <Maximize2 className="h-4 w-4 mr-1" />
              <span className="text-xs">Expand</span>
            </>
          )}
        </Button>
      </div>
    </div>
  );
};
