import React, { useEffect, useRef, useState } from 'react';
import { Database, Loader2, Search } from 'lucide-react';

import type { RowKeySearchMatch } from '@/api/types';
import { useRowKeySearch } from '@/hooks/useLineage';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

interface RowKeyTypeaheadProps {
  value: string;
  onChange: (value: string) => void;
  onSelect: (match: RowKeySearchMatch) => void;
  onSubmit?: (value: string) => void;
  placeholder?: string;
  className?: string;
  minQueryLength?: number;
}

export function RowKeyTypeahead({
  value,
  onChange,
  onSelect,
  onSubmit,
  placeholder = 'Search row keys...',
  className,
  minQueryLength = 2,
}: RowKeyTypeaheadProps) {
  const [debouncedValue, setDebouncedValue] = useState(value);
  const [isOpen, setIsOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const timer = setTimeout(() => {
      setDebouncedValue(value);
    }, 250);

    return () => clearTimeout(timer);
  }, [value]);

  const trimmedQuery = debouncedValue.trim();
  const searchEnabled = trimmedQuery.length >= minQueryLength;
  const { data, isLoading } = useRowKeySearch(
    searchEnabled ? trimmedQuery : undefined,
    { limit: 10 },
    searchEnabled
  );

  const matches = data?.matches ?? [];

  useEffect(() => {
    setSelectedIndex(0);
  }, [matches]);

  useEffect(() => {
    if (matches.length > 0 && value.trim().length >= minQueryLength) {
      setIsOpen(true);
      return;
    }

    if (!isLoading) {
      setIsOpen(false);
    }
  }, [isLoading, matches.length, minQueryLength, value]);

  useEffect(() => {
    const handleClickOutside = (event: MouseEvent) => {
      const target = event.target as Node;
      if (
        dropdownRef.current &&
        !dropdownRef.current.contains(target) &&
        !inputRef.current?.contains(target)
      ) {
        setIsOpen(false);
      }
    };

    document.addEventListener('mousedown', handleClickOutside);
    return () => document.removeEventListener('mousedown', handleClickOutside);
  }, []);

  const submitCurrentValue = () => {
    const nextValue = value.trim();
    if (!nextValue) return;
    onSubmit?.(nextValue);
    setIsOpen(false);
  };

  const handleSelect = (match: RowKeySearchMatch) => {
    onChange(match.row_key);
    onSelect(match);
    setIsOpen(false);
    inputRef.current?.blur();
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Enter') {
      event.preventDefault();
      if (isOpen && matches[selectedIndex]) {
        handleSelect(matches[selectedIndex]);
        return;
      }

      submitCurrentValue();
      return;
    }

    if (!isOpen || matches.length === 0) {
      if (event.key === 'Escape') {
        setIsOpen(false);
      }
      return;
    }

    if (event.key === 'ArrowDown') {
      event.preventDefault();
      setSelectedIndex((current) => (current + 1) % matches.length);
      return;
    }

    if (event.key === 'ArrowUp') {
      event.preventDefault();
      setSelectedIndex((current) => (current - 1 + matches.length) % matches.length);
      return;
    }

    if (event.key === 'Escape') {
      event.preventDefault();
      setIsOpen(false);
    }
  };

  const showEmptyState =
    !isLoading && trimmedQuery.length >= minQueryLength && matches.length === 0;

  return (
    <div className={cn('relative', className)}>
      <div className="relative">
        <Search className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
        <Input
          ref={inputRef}
          type="text"
          value={value}
          onChange={(event) => onChange(event.target.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => {
            if (matches.length > 0) {
              setIsOpen(true);
            }
          }}
          placeholder={placeholder}
          className="pl-9 pr-9"
        />
        {isLoading && (
          <Loader2 className="absolute right-3 top-1/2 h-4 w-4 -translate-y-1/2 animate-spin text-muted-foreground" />
        )}
      </div>

      {(isOpen || showEmptyState) && (
        <div
          ref={dropdownRef}
          className="absolute z-50 mt-2 max-h-80 w-full overflow-hidden rounded-lg border bg-background shadow-xl"
        >
          {matches.length > 0 ? (
            <div className="max-h-80 overflow-y-auto py-1">
              {matches.map((match, index) => (
                <button
                  key={match.row_key}
                  type="button"
                  onClick={() => handleSelect(match)}
                  onMouseEnter={() => setSelectedIndex(index)}
                  className={cn(
                    'flex w-full items-start gap-3 px-3 py-3 text-left transition-colors',
                    'hover:bg-accent hover:text-accent-foreground',
                    selectedIndex === index && 'bg-accent text-accent-foreground'
                  )}
                >
                  <div className="mt-0.5 rounded-md bg-primary/10 p-2 text-primary">
                    <Database className="h-4 w-4" />
                  </div>
                  <div className="min-w-0 flex-1">
                    <div className="truncate font-mono text-sm">{match.row_key}</div>
                    <div className="text-xs text-muted-foreground">
                      {match.source_type} • {match.source_id}
                    </div>
                  </div>
                </button>
              ))}
            </div>
          ) : (
            <div className="px-3 py-4 text-sm text-muted-foreground">
              No matching row keys found yet. Try an Oracle table prefix like
              {' '}
              <span className="font-mono">oracle:CUSTOMER_FEED</span>.
            </div>
          )}
        </div>
      )}
    </div>
  );
}
