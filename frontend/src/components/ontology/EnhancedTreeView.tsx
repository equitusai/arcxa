/**
 * Enhanced Ontology Tree View with Sorting, Filtering, and Controls
 */

import React, { useState, useMemo } from 'react';
import { ClassNode, PropertyNode } from '../../api/ontology';
import { OntologyTreeView } from './OntologyTreeView';
import { TreeControls, SortOption, FilterOption } from './TreeControls';

interface EnhancedTreeViewProps {
  rootClasses: ClassNode[];
  rootProperties: PropertyNode[];
  onNodeClick?: (uri: string, type: 'class' | 'property', node?: ClassNode | PropertyNode) => void;
  searchQuery?: string;
}

export const EnhancedTreeView: React.FC<EnhancedTreeViewProps> = ({
  rootClasses,
  rootProperties,
  onNodeClick,
  searchQuery = '',
}) => {
  const [sortBy, setSortBy] = useState<SortOption>('alpha-asc');
  const [filterBy, setFilterBy] = useState<FilterOption>('all');
  const [allExpanded, setAllExpanded] = useState(false);

  // Count total classes and properties recursively
  const countNodes = useMemo(() => {
    let classCount = 0;
    let propertyCount = 0;

    const countClass = (node: ClassNode) => {
      classCount++;
      if (node.properties) {
        propertyCount += node.properties.length;
      }
      if (node.subclasses) {
        node.subclasses.forEach(countClass);
      }
    };

    rootClasses.forEach(countClass);
    propertyCount += rootProperties.length;

    return { classCount, propertyCount };
  }, [rootClasses, rootProperties]);

  // Sort classes
  const sortClasses = (classes: ClassNode[]): ClassNode[] => {
    const sorted = [...classes];

    switch (sortBy) {
      case 'alpha-asc':
        sorted.sort((a, b) => a.label.localeCompare(b.label));
        break;
      case 'alpha-desc':
        sorted.sort((a, b) => b.label.localeCompare(a.label));
        break;
      case 'depth':
        sorted.sort((a, b) => a.depth - b.depth);
        break;
      case 'properties':
        sorted.sort((a, b) => {
          const aProps = a.properties?.length || 0;
          const bProps = b.properties?.length || 0;
          return bProps - aProps;
        });
        break;
    }

    // Recursively sort subclasses
    return sorted.map((node) => ({
      ...node,
      subclasses: sortClasses(node.subclasses),
    }));
  };

  // Sort properties
  const sortProperties = (properties: PropertyNode[]): PropertyNode[] => {
    const sorted = [...properties];

    switch (sortBy) {
      case 'alpha-asc':
        sorted.sort((a, b) => a.label.localeCompare(b.label));
        break;
      case 'alpha-desc':
        sorted.sort((a, b) => b.label.localeCompare(a.label));
        break;
    }

    return sorted;
  };

  // Filter classes
  const filterClasses = (classes: ClassNode[]): ClassNode[] => {
    return classes
      .filter((node) => {
        // Filter deprecated
        if (filterBy === 'deprecated' && node.deprecated) {
          return false;
        }

        // Search filter
        if (searchQuery) {
          const query = searchQuery.toLowerCase();
          const matchesLabel = node.label.toLowerCase().includes(query);
          const matchesComment = node.comment?.toLowerCase().includes(query);
          const matchesUri = node.uri.toLowerCase().includes(query);

          if (!matchesLabel && !matchesComment && !matchesUri) {
            // Check if any subclass matches
            const hasMatchingSubclass = node.subclasses.some((sub) =>
              filterClasses([sub]).length > 0
            );
            if (!hasMatchingSubclass) return false;
          }
        }

        return true;
      })
      .map((node) => ({
        ...node,
        subclasses: filterClasses(node.subclasses),
        properties:
          filterBy === 'properties'
            ? undefined
            : node.properties
            ? filterProperties(node.properties)
            : undefined,
      }));
  };

  // Filter properties
  const filterProperties = (properties: PropertyNode[]): PropertyNode[] => {
    return properties.filter((node) => {
      // Filter deprecated
      if (filterBy === 'deprecated' && node.deprecated) {
        return false;
      }

      // Search filter
      if (searchQuery) {
        const query = searchQuery.toLowerCase();
        const matchesLabel = node.label.toLowerCase().includes(query);
        const matchesComment = node.comment?.toLowerCase().includes(query);
        const matchesUri = node.uri.toLowerCase().includes(query);

        if (!matchesLabel && !matchesComment && !matchesUri) {
          return false;
        }
      }

      return true;
    });
  };

  // Apply sorting and filtering
  const processedClasses = useMemo(() => {
    if (filterBy === 'properties') return [];
    let classes = rootClasses;
    classes = filterClasses(classes);
    classes = sortClasses(classes);
    return classes;
  }, [rootClasses, sortBy, filterBy, searchQuery]);

  const processedProperties = useMemo(() => {
    if (filterBy === 'classes') return [];
    let properties = rootProperties;
    properties = filterProperties(properties);
    properties = sortProperties(properties);
    return properties;
  }, [rootProperties, sortBy, filterBy, searchQuery]);

  // Count visible nodes
  const countVisibleNodes = useMemo(() => {
    let classCount = 0;
    let propertyCount = 0;

    const countClass = (node: ClassNode) => {
      classCount++;
      if (node.properties) {
        propertyCount += node.properties.length;
      }
      if (node.subclasses) {
        node.subclasses.forEach(countClass);
      }
    };

    processedClasses.forEach(countClass);
    propertyCount += processedProperties.length;

    return { classCount, propertyCount };
  }, [processedClasses, processedProperties]);

  const handleExpandAll = () => {
    setAllExpanded(true);
    // Note: Actual expansion is handled by individual tree nodes
    // We'd need to add a prop to control this
  };

  const handleCollapseAll = () => {
    setAllExpanded(false);
    // Note: Actual collapse is handled by individual tree nodes
  };

  return (
    <div>
      {/* Tree Controls */}
      <TreeControls
        sortBy={sortBy}
        onSortChange={setSortBy}
        filterBy={filterBy}
        onFilterChange={setFilterBy}
        allExpanded={allExpanded}
        onExpandAll={handleExpandAll}
        onCollapseAll={handleCollapseAll}
        totalClasses={countNodes.classCount}
        totalProperties={countNodes.propertyCount}
        visibleClasses={countVisibleNodes.classCount}
        visibleProperties={countVisibleNodes.propertyCount}
      />

      {/* Tree View */}
      <div className="mt-4">
        {processedClasses.length === 0 && processedProperties.length === 0 ? (
          <div className="text-center py-8 text-muted-foreground">
            <p className="text-sm">No items match your filters</p>
            <button
              onClick={() => {
                setFilterBy('all');
                setSortBy('alpha-asc');
              }}
              className="mt-2 text-xs text-primary hover:underline"
            >
              Clear filters
            </button>
          </div>
        ) : (
          <OntologyTreeView
            rootClasses={processedClasses}
            rootProperties={processedProperties}
            showProperties={filterBy !== 'properties'}
            onNodeClick={onNodeClick}
          />
        )}
      </div>
    </div>
  );
};
