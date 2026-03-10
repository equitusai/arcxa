/**
 * Global Ontology Search Component
 *
 * Fuzzy search across all classes and properties with instant results
 */

import React, { useState, useEffect, useRef, useMemo } from 'react';
import Fuse from 'fuse.js';
import { Search, Box, Link as LinkIcon, Database, X } from 'lucide-react';
import { ClassNode, PropertyNode } from '../../api/ontology';

interface SearchableItem {
  uri: string;
  label: string;
  comment?: string;
  type: 'class' | 'property';
  propertyType?: 'object_property' | 'datatype_property' | 'annotation_property';
  node: ClassNode | PropertyNode;
}

interface GlobalSearchProps {
  rootClasses: ClassNode[];
  rootProperties: PropertyNode[];
  onSelectResult: (uri: string, type: 'class' | 'property', node: ClassNode | PropertyNode) => void;
  placeholder?: string;
}

export const GlobalSearch: React.FC<GlobalSearchProps> = ({
  rootClasses,
  rootProperties,
  onSelectResult,
  placeholder = "Search classes, properties, URIs...",
}) => {
  const [query, setQuery] = useState('');
  const [results, setResults] = useState<SearchableItem[]>([]);
  const [isOpen, setIsOpen] = useState(false);
  const [selectedIndex, setSelectedIndex] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const dropdownRef = useRef<HTMLDivElement>(null);

  // Build searchable index
  const searchIndex = useMemo(() => {
    const items: SearchableItem[] = [];

    // Recursively extract all classes
    const extractClasses = (classes: ClassNode[]) => {
      classes.forEach((cls) => {
        items.push({
          uri: cls.uri,
          label: cls.label,
          comment: cls.comment,
          type: 'class',
          node: cls,
        });

        // Add properties of this class
        if (cls.properties) {
          cls.properties.forEach((prop) => {
            items.push({
              uri: prop.uri,
              label: prop.label,
              comment: prop.comment,
              type: 'property',
              propertyType: prop.property_type,
              node: prop,
            });
          });
        }

        // Recurse into subclasses
        if (cls.subclasses) {
          extractClasses(cls.subclasses);
        }
      });
    };

    extractClasses(rootClasses);

    // Add root properties
    rootProperties.forEach((prop) => {
      items.push({
        uri: prop.uri,
        label: prop.label,
        comment: prop.comment,
        type: 'property',
        propertyType: prop.property_type,
        node: prop,
      });
    });

    return items;
  }, [rootClasses, rootProperties]);

  // Initialize Fuse.js
  const fuse = useMemo(() => {
    return new Fuse(searchIndex, {
      keys: [
        { name: 'label', weight: 2 },
        { name: 'uri', weight: 1 },
        { name: 'comment', weight: 0.5 },
      ],
      threshold: 0.3,
      includeScore: true,
      minMatchCharLength: 2,
    });
  }, [searchIndex]);

  // Perform search
  useEffect(() => {
    if (query.trim().length < 2) {
      setResults([]);
      setIsOpen(false);
      return;
    }

    // Type-specific filtering
    let filtered = searchIndex;
    let searchQuery = query;

    if (query.startsWith('class:')) {
      searchQuery = query.substring(6);
      filtered = searchIndex.filter((item) => item.type === 'class');
    } else if (query.startsWith('property:')) {
      searchQuery = query.substring(9);
      filtered = searchIndex.filter((item) => item.type === 'property');
    }

    if (searchQuery.trim().length < 2) {
      setResults([]);
      setIsOpen(false);
      return;
    }

    const fuseResults = new Fuse(filtered, {
      keys: [
        { name: 'label', weight: 2 },
        { name: 'uri', weight: 1 },
        { name: 'comment', weight: 0.5 },
      ],
      threshold: 0.3,
      includeScore: true,
      minMatchCharLength: 2,
    }).search(searchQuery);

    const topResults = fuseResults.slice(0, 10).map((result) => result.item);
    setResults(topResults);
    setIsOpen(topResults.length > 0);
    setSelectedIndex(0);
  }, [query, searchIndex]);

  // Keyboard navigation
  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (!isOpen) return;

    switch (e.key) {
      case 'ArrowDown':
        e.preventDefault();
        setSelectedIndex((prev) => (prev + 1) % results.length);
        break;
      case 'ArrowUp':
        e.preventDefault();
        setSelectedIndex((prev) => (prev - 1 + results.length) % results.length);
        break;
      case 'Enter':
        e.preventDefault();
        if (results[selectedIndex]) {
          handleSelectResult(results[selectedIndex]);
        }
        break;
      case 'Escape':
        e.preventDefault();
        setIsOpen(false);
        setQuery('');
        break;
    }
  };

  // Select result
  const handleSelectResult = (item: SearchableItem) => {
    onSelectResult(item.uri, item.type, item.node);
    setQuery('');
    setIsOpen(false);
    inputRef.current?.blur();
  };

  // Click outside to close
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

  // Clear search
  const handleClear = () => {
    setQuery('');
    setResults([]);
    setIsOpen(false);
    inputRef.current?.focus();
  };

  const extractLocalName = (uri: string): string => {
    const parts = uri.split(/[#/]/);
    return parts[parts.length - 1] || uri;
  };

  return (
    <div className="relative">
      {/* Search Input */}
      <div className="relative">
        <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground pointer-events-none" />
        <input
          ref={inputRef}
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={handleKeyDown}
          onFocus={() => query.length >= 2 && results.length > 0 && setIsOpen(true)}
          placeholder={placeholder}
          className="h-9 w-full rounded-sm border border-border bg-background pl-9 pr-9 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-accent focus:border-accent transition-colors"
        />
        {query && (
          <button
            onClick={handleClear}
            className="absolute right-2 top-1/2 -translate-y-1/2 p-1 hover:bg-muted rounded"
          >
            <X className="h-3.5 w-3.5 text-muted-foreground" />
          </button>
        )}
      </div>

      {/* Search Results Dropdown */}
      {isOpen && results.length > 0 && (
        <div
          ref={dropdownRef}
          className="absolute top-full left-0 right-0 mt-2 bg-card border border-border rounded-lg shadow-lg overflow-hidden z-50 max-h-[400px] overflow-y-auto"
        >
          {/* Group results by type */}
          {(() => {
            const classes = results.filter((r) => r.type === 'class');
            const properties = results.filter((r) => r.type === 'property');

            return (
              <>
                {classes.length > 0 && (
                  <div>
                    <div className="px-3 py-2 bg-muted/50 border-b border-border">
                      <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                        Classes ({classes.length})
                      </p>
                    </div>
                    {classes.map((item, idx) => {
                      const globalIdx = results.indexOf(item);
                      return (
                        <SearchResultItem
                          key={item.uri}
                          item={item}
                          isSelected={globalIdx === selectedIndex}
                          onClick={() => handleSelectResult(item)}
                          extractLocalName={extractLocalName}
                        />
                      );
                    })}
                  </div>
                )}

                {properties.length > 0 && (
                  <div>
                    <div className="px-3 py-2 bg-muted/50 border-b border-border">
                      <p className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                        Properties ({properties.length})
                      </p>
                    </div>
                    {properties.map((item, idx) => {
                      const globalIdx = results.indexOf(item);
                      return (
                        <SearchResultItem
                          key={item.uri}
                          item={item}
                          isSelected={globalIdx === selectedIndex}
                          onClick={() => handleSelectResult(item)}
                          extractLocalName={extractLocalName}
                        />
                      );
                    })}
                  </div>
                )}
              </>
            );
          })()}
        </div>
      )}

      {/* Search Tips */}
      {query.length > 0 && query.length < 2 && (
        <div className="absolute top-full left-0 right-0 mt-2 px-3 py-2 bg-muted/50 border border-border rounded-lg text-xs text-muted-foreground">
          Type at least 2 characters to search. Try <code className="px-1 py-0.5 bg-background rounded">class:</code> or <code className="px-1 py-0.5 bg-background rounded">property:</code> to filter.
        </div>
      )}
    </div>
  );
};

/**
 * Search Result Item Component
 */
interface SearchResultItemProps {
  item: SearchableItem;
  isSelected: boolean;
  onClick: () => void;
  extractLocalName: (uri: string) => string;
}

const SearchResultItem: React.FC<SearchResultItemProps> = ({
  item,
  isSelected,
  onClick,
  extractLocalName,
}) => {
  return (
    <div
      onClick={onClick}
      className={`px-3 py-2.5 cursor-pointer border-b border-border last:border-b-0 transition-colors ${
        isSelected ? 'bg-accent' : 'hover:bg-muted/50'
      }`}
    >
      <div className="flex items-start gap-2">
        {/* Icon */}
        {item.type === 'class' ? (
          <Box className="h-4 w-4 text-blue-600 flex-shrink-0 mt-0.5" />
        ) : item.propertyType === 'object_property' ? (
          <LinkIcon className="h-4 w-4 text-green-600 flex-shrink-0 mt-0.5" />
        ) : (
          <Database className="h-4 w-4 text-purple-600 flex-shrink-0 mt-0.5" />
        )}

        {/* Content */}
        <div className="flex-1 min-w-0">
          <div className="flex items-center justify-between gap-2">
            <p className="text-sm font-medium truncate">{item.label}</p>
            <span className="text-xs text-muted-foreground font-mono flex-shrink-0">
              {extractLocalName(item.uri)}
            </span>
          </div>
          {item.comment && (
            <p className="text-xs text-muted-foreground truncate mt-0.5">{item.comment}</p>
          )}
        </div>
      </div>
    </div>
  );
};
