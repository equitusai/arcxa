/**
 * EntityTypeahead Component
 * Premium typeahead search for finding entities to view lineage
 *
 * Features:
 * - Live search with debouncing
 * - Beautiful dropdown with entity metadata
 * - Keyboard navigation (arrows, enter, escape)
 * - Loading and empty states
 * - Premium Oracle Redwood × Microsoft Fluent design
 */

import React, { useState, useEffect, useRef, useMemo } from 'react';
import { Search, Loader2, Database, AlertCircle } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { cn } from '@/lib/utils';
import { entityApi } from '@/api/entities';
import { useQuery } from '@tanstack/react-query';
import { Input } from '@/components/ui/input';
import type { Entity as ApiEntity } from '@/types/entity';

interface SearchEntity {
  id: string;
  label: string;
  type?: string;
  domain?: string;
  description?: string;
  dataset?: string;
}

interface EntityTypeaheadProps {
  value: string;
  onChange: (value: string) => void;
  onSelect: (entity: SearchEntity) => void;
  placeholder?: string;
  className?: string;
}

function getDerivedAttribute(entity: ApiEntity, ...names: string[]): string | undefined {
  const matchedAttribute = entity.attributes.find((attribute) =>
    names.includes(attribute.name.toLowerCase())
  );

  if (matchedAttribute?.value === undefined || matchedAttribute?.value === null) {
    return undefined;
  }

  return String(matchedAttribute.value);
}

function mapEntityToSearchEntity(entity: ApiEntity): SearchEntity {
  return {
    id: entity.id,
    label:
      getDerivedAttribute(entity, 'label', 'name', 'title', 'display_name') || entity.id,
    type: getDerivedAttribute(entity, 'type', 'entity_type') || entity.domain,
    domain: entity.domain,
    description: getDerivedAttribute(entity, 'description', 'summary'),
    dataset: entity.source_systems?.[0],
  };
}

export function EntityTypeahead({
  value,
  onChange,
  onSelect,
  placeholder = 'Search for entities...',
  className,
}: EntityTypeaheadProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [debouncedValue, setDebouncedValue] = useState(value);
  const inputRef = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Debounce search input
  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedValue(value);
    }, 300);

    return () => clearTimeout(timer);
  }, [value]);

  // Search entities using the API
  const { data: searchResults, isLoading } = useQuery({
    queryKey: ['entities-search', debouncedValue],
    queryFn: async () => {
      if (!debouncedValue || debouncedValue.length < 2) {
        return { data: [], total: 0 };
      }
      return entityApi.list({
        search: debouncedValue,
        limit: 10,
      });
    },
    enabled: debouncedValue.length >= 2,
    staleTime: 30000, // 30 seconds
  });

  const entities = useMemo<SearchEntity[]>(
    () => (searchResults?.data || []).map(mapEntityToSearchEntity),
    [searchResults]
  );

  // Open dropdown when there are results
  useEffect(() => {
    if (entities.length > 0 && value.length >= 2) {
      setIsOpen(true);
    } else {
      setIsOpen(false);
    }
  }, [entities, value]);

  // Reset selected index when results change
  useEffect(() => {
    setSelectedIndex(0);
  }, [entities]);

  // Handle keyboard navigation
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!isOpen || entities.length === 0) {
      if (e.key === 'Enter' && value) {
        // Allow submitting even without selecting from dropdown
        e.preventDefault();
      }
      return;
    }

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % entities.length);
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + entities.length) % entities.length);
        break;
      case 'Enter':
        e.preventDefault();
        if (entities[selectedIndex]) {
          handleSelect(entities[selectedIndex]);
        }
        break;
      case 'Escape':
        e.preventDefault();
        setIsOpen(false);
        inputRef.current?.blur();
        break;
    }
  };

  // Handle entity selection
  const handleSelect = (entity: SearchEntity) => {
    onChange(entity.id);
    onSelect(entity);
    setIsOpen(false);
    inputRef.current?.blur();
  };

  // Close dropdown when clicking outside
  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(event.target as Node) &&
        !inputRef.current?.contains(event.target as Node)
      ) {
        setIsOpen(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  return (
    <div className={cn('relative', className)}>
      {/* Search Input */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground pointer-events-none" />
        <Input
          ref={inputRef}
          type="text"
          value={value}
          onChange={(e) => onChange(e.target.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => {
            if (entities.length > 0 && value.length >= 2) {
              setIsOpen(true);
            }
          }}
          placeholder={placeholder}
          className="pl-9 pr-9"
        />
        {isLoading && (
          <Loader2 className="absolute right-3 top-1/2 -translate-y-1/2 h-4 w-4 text-muted-foreground animate-spin" />
        )}
      </div>

      {/* Dropdown */}
      <AnimatePresence>
        {isOpen && (
          <motion.div
            ref={dropdownRef}
            initial={{ opacity: 0, y: -8 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, y: -8 }}
            transition={{ duration: 0.15 }}
            className="absolute z-50 w-full mt-2 bg-white dark:bg-neutral-800 border border-neutral-200 dark:border-neutral-700 rounded-lg shadow-xl dark:shadow-2xl overflow-hidden"
          >
            {/* Results */}
            {entities.length > 0 ? (
              <div className="max-h-80 overflow-y-auto">
                {entities.map((entity, index) => (
                  <button
                    key={entity.id}
                    onClick={() => handleSelect(entity)}
                    onMouseEnter={() => setSelectedIndex(index)}
                    className={cn(
                      'w-full px-4 py-3 text-left transition-colors',
                      'hover:bg-blue-50 dark:hover:bg-blue-950/30',
                      'focus:outline-none focus:bg-blue-50 dark:focus:bg-blue-950/30',
                      selectedIndex === index && 'bg-blue-50 dark:bg-blue-950/30'
                    )}
                  >
                    <div className="flex items-start gap-3">
                      {/* Icon */}
                      <div className="flex-shrink-0 mt-0.5">
                        <div className="p-2 bg-blue-100 dark:bg-blue-900/30 rounded-md">
                          <Database className="w-4 h-4 text-blue-600 dark:text-blue-400" />
                        </div>
                      </div>

                      {/* Entity Info */}
                      <div className="flex-1 min-w-0">
                        <div className="flex items-center gap-2 mb-0.5">
                          <span className="font-semibold text-neutral-900 dark:text-neutral-50 truncate">
                            {entity.label || entity.id}
                          </span>
                          {entity.type && (
                            <span className="px-2 py-0.5 text-[9px] font-semibold uppercase tracking-wide bg-neutral-200 dark:bg-neutral-700 text-neutral-600 dark:text-neutral-400 rounded flex-shrink-0">
                              {entity.type}
                            </span>
                          )}
                        </div>

                        {entity.description && (
                          <p className="text-xs text-neutral-600 dark:text-neutral-400 line-clamp-1 mb-1">
                            {entity.description}
                          </p>
                        )}

                        <div className="flex items-center gap-3 text-[10px] text-neutral-500 dark:text-neutral-500">
                          {entity.domain && (
                            <span>Domain: {entity.domain}</span>
                          )}
                          {entity.dataset && (
                            <span>Dataset: {entity.dataset}</span>
                          )}
                          <span className="font-mono">{entity.id}</span>
                        </div>
                      </div>
                    </div>
                  </button>
                ))}
              </div>
            ) : (
              // Empty state
              <div className="px-4 py-8 text-center">
                <AlertCircle className="w-8 h-8 text-neutral-400 dark:text-neutral-600 mx-auto mb-2" />
                <p className="text-sm font-medium text-neutral-900 dark:text-neutral-50 mb-1">
                  No entities found
                </p>
                <p className="text-xs text-neutral-600 dark:text-neutral-400">
                  Try a different search term
                </p>
              </div>
            )}

            {/* Footer hint */}
            {entities.length > 0 && (
              <div className="px-4 py-2 bg-neutral-50 dark:bg-neutral-900 border-t border-neutral-200 dark:border-neutral-700">
                <p className="text-[10px] text-neutral-500 dark:text-neutral-500">
                  Use ↑↓ to navigate • Enter to select • Esc to close
                </p>
              </div>
            )}
          </motion.div>
        )}
      </AnimatePresence>
    </div>
  );
}
