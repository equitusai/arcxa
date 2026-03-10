/**
 * Ontology Tree View Component
 *
 * Displays ontology class and property hierarchies as an expandable tree
 */

import React, { useState } from 'react';
import { ChevronRight, ChevronDown, Circle, Box, Database, Link as LinkIcon } from 'lucide-react';
import { ClassNode, PropertyNode, PropertyType } from '../../api/ontology';

interface OntologyTreeViewProps {
  rootClasses: ClassNode[];
  rootProperties: PropertyNode[];
  showProperties?: boolean;
  onNodeClick?: (uri: string, type: 'class' | 'property') => void;
}

export const OntologyTreeView: React.FC<OntologyTreeViewProps> = ({
  rootClasses,
  rootProperties,
  showProperties = true,
  onNodeClick,
}) => {
  return (
    <div className="space-y-4">
      {rootClasses.length > 0 && (
        <div>
          <h3 className="text-sm font-semibold text-muted-foreground mb-2 flex items-center gap-2">
            <Box className="h-5 w-5" />
            Classes
          </h3>
          <div className="border rounded-lg p-3 bg-card">
            {rootClasses.map((classNode) => (
              <ClassTreeNode
                key={classNode.uri}
                node={classNode}
                showProperties={showProperties}
                onNodeClick={onNodeClick}
              />
            ))}
          </div>
        </div>
      )}

      {showProperties && rootProperties.length > 0 && (
        <div>
          <h3 className="text-sm font-semibold text-muted-foreground mb-2 flex items-center gap-2">
            <LinkIcon className="h-5 w-5" />
            Properties
          </h3>
          <div className="border rounded-lg p-3 bg-card">
            {rootProperties.map((propertyNode) => (
              <PropertyTreeNode
                key={propertyNode.uri}
                node={propertyNode}
                onNodeClick={onNodeClick}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
};

/**
 * Class node in the tree
 */
interface ClassTreeNodeProps {
  node: ClassNode;
  showProperties: boolean;
  onNodeClick?: (uri: string, type: 'class' | 'property') => void;
  level?: number;
}

const ClassTreeNode: React.FC<ClassTreeNodeProps> = ({
  node,
  showProperties,
  onNodeClick,
  level = 0,
}) => {
  const [isExpanded, setIsExpanded] = useState(level < 2); // Auto-expand first 2 levels
  const hasChildren = node.subclasses.length > 0 || (showProperties && node.properties && node.properties.length > 0);

  const handleClick = () => {
    if (onNodeClick) {
      onNodeClick(node.uri, 'class');
    }
  };

  return (
    <div className="select-none">
      <div
        className={`flex items-center gap-2 py-1 px-2 rounded hover:bg-muted/50 cursor-pointer ${
          node.deprecated ? 'opacity-50' : ''
        }`}
        style={{ paddingLeft: `${level * 1.5 + 0.5}rem` }}
        onClick={() => setIsExpanded(!isExpanded)}
      >
        {hasChildren ? (
          isExpanded ? (
            <ChevronDown className="h-5 w-5 text-muted-foreground flex-shrink-0" />
          ) : (
            <ChevronRight className="h-5 w-5 text-muted-foreground flex-shrink-0" />
          )
        ) : (
          <Circle className="h-4 w-4 text-muted-foreground flex-shrink-0 ml-0.5" />
        )}

        <Box className="h-5 w-5 text-blue-600 flex-shrink-0" />

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span
              className="font-medium text-sm truncate hover:underline"
              onClick={(e) => {
                e.stopPropagation();
                handleClick();
              }}
            >
              {node.label}
            </span>
            {node.deprecated && (
              <span className="text-xs text-muted-foreground">(deprecated)</span>
            )}
          </div>
          {node.comment && (
            <p className="text-xs text-muted-foreground truncate">{node.comment}</p>
          )}
        </div>
      </div>

      {isExpanded && hasChildren && (
        <div>
          {/* Show properties of this class */}
          {showProperties && node.properties && node.properties.length > 0 && (
            <div className="border-l-2 border-muted ml-3">
              {node.properties.map((prop) => (
                <PropertyTreeNode
                  key={prop.uri}
                  node={prop}
                  onNodeClick={onNodeClick}
                  level={level + 1}
                  compact={true}
                />
              ))}
            </div>
          )}

          {/* Show subclasses */}
          {node.subclasses.length > 0 && (
            <div className="border-l-2 border-muted ml-3">
              {node.subclasses.map((subclass) => (
                <ClassTreeNode
                  key={subclass.uri}
                  node={subclass}
                  showProperties={showProperties}
                  onNodeClick={onNodeClick}
                  level={level + 1}
                />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

/**
 * Property node in the tree
 */
interface PropertyTreeNodeProps {
  node: PropertyNode;
  onNodeClick?: (uri: string, type: 'class' | 'property') => void;
  level?: number;
  compact?: boolean;
}

const PropertyTreeNode: React.FC<PropertyTreeNodeProps> = ({
  node,
  onNodeClick,
  level = 0,
  compact = false,
}) => {
  const [isExpanded, setIsExpanded] = useState(false);
  const hasChildren = node.subproperties.length > 0;

  const getPropertyIcon = (type: PropertyType) => {
    switch (type) {
      case 'object_property':
        return <LinkIcon className="h-5 w-5 text-green-600" />;
      case 'datatype_property':
        return <Database className="h-5 w-5 text-purple-600" />;
      default:
        return <Circle className="h-5 w-5 text-gray-600" />;
    }
  };

  const handleClick = () => {
    if (onNodeClick) {
      onNodeClick(node.uri, 'property');
    }
  };

  return (
    <div className="select-none">
      <div
        className={`flex items-center gap-2 py-1 px-2 rounded hover:bg-muted/50 cursor-pointer ${
          node.deprecated ? 'opacity-50' : ''
        }`}
        style={{ paddingLeft: `${level * 1.5 + 0.5}rem` }}
        onClick={() => hasChildren && setIsExpanded(!isExpanded)}
      >
        {hasChildren ? (
          isExpanded ? (
            <ChevronDown className="h-5 w-5 text-muted-foreground flex-shrink-0" />
          ) : (
            <ChevronRight className="h-5 w-5 text-muted-foreground flex-shrink-0" />
          )
        ) : (
          <Circle className="h-4 w-4 text-muted-foreground flex-shrink-0 ml-0.5" />
        )}

        {getPropertyIcon(node.property_type)}

        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span
              className="font-medium text-sm truncate hover:underline"
              onClick={(e) => {
                e.stopPropagation();
                handleClick();
              }}
            >
              {node.label}
            </span>
            {node.deprecated && (
              <span className="text-xs text-muted-foreground">(deprecated)</span>
            )}
          </div>
          {!compact && node.comment && (
            <p className="text-xs text-muted-foreground truncate">{node.comment}</p>
          )}
          {!compact && (node.domain.length > 0 || node.range.length > 0) && (
            <div className="text-xs text-muted-foreground mt-0.5">
              {node.domain.length > 0 && (
                <span className="mr-2">
                  Domain: {node.domain.map(extractLocalName).join(', ')}
                </span>
              )}
              {node.range.length > 0 && (
                <span>
                  Range: {node.range.map(extractLocalName).join(', ')}
                </span>
              )}
            </div>
          )}
        </div>
      </div>

      {isExpanded && hasChildren && (
        <div className="border-l-2 border-muted ml-3">
          {node.subproperties.map((subprop) => (
            <PropertyTreeNode
              key={subprop.uri}
              node={subprop}
              onNodeClick={onNodeClick}
              level={level + 1}
              compact={compact}
            />
          ))}
        </div>
      )}
    </div>
  );
};

/**
 * Extract local name from URI
 */
function extractLocalName(uri: string): string {
  const parts = uri.split(/[#/]/);
  return parts[parts.length - 1] || uri;
}
