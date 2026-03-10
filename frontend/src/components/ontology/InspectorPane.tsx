/**
 * Enhanced Inspector Pane
 *
 * Slides in from the right to show detailed information about selected classes or properties
 */

import React, { useState } from 'react';
import { X, Copy, ExternalLink, Box, Link as LinkIcon, Database, Info, Network, Tag } from 'lucide-react';
import { ClassNode, PropertyNode } from '../../api/ontology';
import { Button } from '../ui/button';

interface InspectorPaneProps {
  selectedNode: ClassNode | PropertyNode | null;
  nodeType: 'class' | 'property' | null;
  onClose: () => void;
}

export const InspectorPane: React.FC<InspectorPaneProps> = ({
  selectedNode,
  nodeType,
  onClose,
}) => {
  const [activeTab, setActiveTab] = useState<'details' | 'relationships' | 'usage' | 'metadata'>('details');

  if (!selectedNode) return null;

  const copyUri = () => {
    navigator.clipboard.writeText(selectedNode.uri);
  };

  const extractLocalName = (uri: string): string => {
    const parts = uri.split(/[#/]/);
    return parts[parts.length - 1] || uri;
  };

  return (
    <div className="fixed right-0 top-0 h-screen w-[420px] bg-card border-l border-border shadow-xl z-50 flex flex-col">
      {/* Header */}
      <div className="p-4 border-b border-border flex items-start justify-between">
        <div className="flex items-start gap-3 flex-1 min-w-0">
          {nodeType === 'class' ? (
            <Box className="h-5 w-5 text-blue-600 flex-shrink-0 mt-0.5" />
          ) : (
            <LinkIcon className="h-5 w-5 text-green-600 flex-shrink-0 mt-0.5" />
          )}
          <div className="flex-1 min-w-0">
            <h2 className="text-lg font-semibold truncate">{selectedNode.label}</h2>
            <p className="text-xs text-muted-foreground font-mono truncate">{nodeType}</p>
          </div>
        </div>
        <Button variant="ghost" size="icon" onClick={onClose} className="flex-shrink-0">
          <X className="h-4 w-4" />
        </Button>
      </div>

      {/* URI Bar */}
      <div className="px-4 py-3 bg-muted/50 border-b border-border">
        <div className="flex items-center gap-2">
          <code className="flex-1 text-xs font-mono truncate">{selectedNode.uri}</code>
          <Button variant="ghost" size="sm" onClick={copyUri} className="flex-shrink-0">
            <Copy className="h-3 w-3 mr-1" />
            Copy
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => window.open(selectedNode.uri, '_blank')}
            className="flex-shrink-0"
          >
            <ExternalLink className="h-3 w-3" />
          </Button>
        </div>
      </div>

      {/* Tabs */}
      <div className="flex border-b border-border bg-background">
        <TabButton
          active={activeTab === 'details'}
          onClick={() => setActiveTab('details')}
          icon={<Info className="h-3.5 w-3.5" />}
        >
          Details
        </TabButton>
        <TabButton
          active={activeTab === 'relationships'}
          onClick={() => setActiveTab('relationships')}
          icon={<Network className="h-3.5 w-3.5" />}
        >
          Relationships
        </TabButton>
        {nodeType === 'property' && (
          <TabButton
            active={activeTab === 'usage'}
            onClick={() => setActiveTab('usage')}
            icon={<Database className="h-3.5 w-3.5" />}
          >
            Usage
          </TabButton>
        )}
        <TabButton
          active={activeTab === 'metadata'}
          onClick={() => setActiveTab('metadata')}
          icon={<Tag className="h-3.5 w-3.5" />}
        >
          Metadata
        </TabButton>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto p-4">
        {activeTab === 'details' && (
          <DetailsTab node={selectedNode} nodeType={nodeType} />
        )}
        {activeTab === 'relationships' && (
          <RelationshipsTab node={selectedNode} nodeType={nodeType} />
        )}
        {activeTab === 'usage' && nodeType === 'property' && (
          <UsageTab node={selectedNode as PropertyNode} />
        )}
        {activeTab === 'metadata' && (
          <MetadataTab node={selectedNode} />
        )}
      </div>
    </div>
  );
};

/**
 * Tab Button Component
 */
interface TabButtonProps {
  active: boolean;
  onClick: () => void;
  icon: React.ReactNode;
  children: React.ReactNode;
}

const TabButton: React.FC<TabButtonProps> = ({ active, onClick, icon, children }) => (
  <button
    onClick={onClick}
    className={`flex items-center gap-2 px-4 py-2.5 text-sm font-medium border-b-2 transition-colors ${
      active
        ? 'border-primary text-foreground bg-background'
        : 'border-transparent text-muted-foreground hover:text-foreground hover:bg-muted/50'
    }`}
  >
    {icon}
    {children}
  </button>
);

/**
 * Details Tab
 */
interface DetailsTabProps {
  node: ClassNode | PropertyNode;
  nodeType: 'class' | 'property' | null;
}

const DetailsTab: React.FC<DetailsTabProps> = ({ node, nodeType }) => {
  const extractLocalName = (uri: string): string => {
    const parts = uri.split(/[#/]/);
    return parts[parts.length - 1] || uri;
  };

  return (
    <div className="space-y-6">
      {/* Basic Info */}
      <Section title="Basic Information">
        <InfoRow label="Label" value={node.label} />
        {node.comment && <InfoRow label="Comment" value={node.comment} multiline />}
        {node.deprecated && (
          <div className="bg-yellow-50 border border-yellow-200 rounded-lg p-3">
            <p className="text-sm text-yellow-800 font-medium">⚠️ This {nodeType} is deprecated</p>
          </div>
        )}
      </Section>

      {/* Class-specific properties */}
      {nodeType === 'class' && 'subclasses' in node && (
        <>
          {node.properties && node.properties.length > 0 && (
            <Section title={`Properties (${node.properties.length})`}>
              <div className="space-y-2">
                {node.properties.map((prop) => (
                  <PropertyItem key={prop.uri} property={prop} />
                ))}
              </div>
            </Section>
          )}
        </>
      )}

      {/* Property-specific details */}
      {nodeType === 'property' && 'property_type' in node && (
        <Section title="Property Type">
          <div className="flex items-center gap-2">
            {node.property_type === 'object_property' ? (
              <>
                <LinkIcon className="h-4 w-4 text-green-600" />
                <span className="text-sm font-medium">Object Property</span>
              </>
            ) : (
              <>
                <Database className="h-4 w-4 text-purple-600" />
                <span className="text-sm font-medium">Datatype Property</span>
              </>
            )}
          </div>
        </Section>
      )}
    </div>
  );
};

/**
 * Relationships Tab
 */
interface RelationshipsTabProps {
  node: ClassNode | PropertyNode;
  nodeType: 'class' | 'property' | null;
}

const RelationshipsTab: React.FC<RelationshipsTabProps> = ({ node, nodeType }) => {
  const extractLocalName = (uri: string): string => {
    const parts = uri.split(/[#/]/);
    return parts[parts.length - 1] || uri;
  };

  return (
    <div className="space-y-6">
      {/* Parent Classes */}
      {nodeType === 'class' && 'parent_classes' in node && node.parent_classes.length > 0 && (
        <Section title="Parent Classes">
          <div className="space-y-1">
            {node.parent_classes.map((uri) => (
              <div key={uri} className="text-sm px-2 py-1.5 bg-muted/50 rounded">
                <code className="text-xs font-mono">{extractLocalName(uri)}</code>
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Subclasses */}
      {nodeType === 'class' && 'subclasses' in node && node.subclasses.length > 0 && (
        <Section title={`Subclasses (${node.subclasses.length})`}>
          <div className="space-y-1">
            {node.subclasses.map((subclass) => (
              <div key={subclass.uri} className="flex items-center gap-2 text-sm px-2 py-1.5 hover:bg-muted/50 rounded">
                <Box className="h-3.5 w-3.5 text-blue-600 flex-shrink-0" />
                <span className="font-medium">{subclass.label}</span>
                {subclass.subclasses.length > 0 && (
                  <span className="text-xs text-muted-foreground ml-auto">
                    {subclass.subclasses.length} subclass{subclass.subclasses.length > 1 ? 'es' : ''}
                  </span>
                )}
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Parent Properties */}
      {nodeType === 'property' && 'parent_properties' in node && node.parent_properties.length > 0 && (
        <Section title="Parent Properties">
          <div className="space-y-1">
            {node.parent_properties.map((uri) => (
              <div key={uri} className="text-sm px-2 py-1.5 bg-muted/50 rounded">
                <code className="text-xs font-mono">{extractLocalName(uri)}</code>
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Subproperties */}
      {nodeType === 'property' && 'subproperties' in node && node.subproperties.length > 0 && (
        <Section title={`Subproperties (${node.subproperties.length})`}>
          <div className="space-y-1">
            {node.subproperties.map((subprop) => (
              <div key={subprop.uri} className="flex items-center gap-2 text-sm px-2 py-1.5 hover:bg-muted/50 rounded">
                <LinkIcon className="h-3.5 w-3.5 text-green-600 flex-shrink-0" />
                <span className="font-medium">{subprop.label}</span>
              </div>
            ))}
          </div>
        </Section>
      )}
    </div>
  );
};

/**
 * Usage Tab (for properties)
 */
interface UsageTabProps {
  node: PropertyNode;
}

const UsageTab: React.FC<UsageTabProps> = ({ node }) => {
  const extractLocalName = (uri: string): string => {
    const parts = uri.split(/[#/]/);
    return parts[parts.length - 1] || uri;
  };

  return (
    <div className="space-y-6">
      {/* Domain */}
      {node.domain.length > 0 && (
        <Section title="Domain">
          <p className="text-sm text-muted-foreground mb-2">
            Classes that can have this property
          </p>
          <div className="space-y-1">
            {node.domain.map((uri) => (
              <div key={uri} className="text-sm px-2 py-1.5 bg-blue-50 border border-blue-200 rounded">
                <code className="text-xs font-mono text-blue-900">{extractLocalName(uri)}</code>
              </div>
            ))}
          </div>
        </Section>
      )}

      {/* Range */}
      {node.range.length > 0 && (
        <Section title="Range">
          <p className="text-sm text-muted-foreground mb-2">
            Possible value types
          </p>
          <div className="space-y-1">
            {node.range.map((uri) => (
              <div key={uri} className="text-sm px-2 py-1.5 bg-purple-50 border border-purple-200 rounded">
                <code className="text-xs font-mono text-purple-900">{extractLocalName(uri)}</code>
              </div>
            ))}
          </div>
        </Section>
      )}
    </div>
  );
};

/**
 * Metadata Tab
 */
interface MetadataTabProps {
  node: ClassNode | PropertyNode;
}

const MetadataTab: React.FC<MetadataTabProps> = ({ node }) => {
  return (
    <div className="space-y-6">
      <Section title="Identifiers">
        <InfoRow label="URI" value={node.uri} multiline monospace />
        <InfoRow label="Local Name" value={node.label} />
      </Section>

      <Section title="Hierarchy">
        {'depth' in node && <InfoRow label="Depth" value={node.depth.toString()} />}
      </Section>

      <Section title="Status">
        <InfoRow
          label="Deprecated"
          value={node.deprecated ? 'Yes' : 'No'}
          highlight={node.deprecated ? 'warning' : undefined}
        />
      </Section>
    </div>
  );
};

/**
 * Helper Components
 */
interface SectionProps {
  title: string;
  children: React.ReactNode;
}

const Section: React.FC<SectionProps> = ({ title, children }) => (
  <div>
    <h3 className="text-sm font-semibold text-foreground mb-3">{title}</h3>
    {children}
  </div>
);

interface InfoRowProps {
  label: string;
  value: string;
  multiline?: boolean;
  monospace?: boolean;
  highlight?: 'warning' | 'error' | 'success';
}

const InfoRow: React.FC<InfoRowProps> = ({ label, value, multiline, monospace, highlight }) => (
  <div className="mb-3">
    <dt className="text-xs font-medium text-muted-foreground mb-1">{label}</dt>
    <dd
      className={`text-sm ${monospace ? 'font-mono' : ''} ${
        multiline ? '' : 'truncate'
      } ${highlight === 'warning' ? 'text-yellow-700 font-medium' : ''}`}
    >
      {value}
    </dd>
  </div>
);

interface PropertyItemProps {
  property: PropertyNode;
}

const PropertyItem: React.FC<PropertyItemProps> = ({ property }) => {
  const extractLocalName = (uri: string): string => {
    const parts = uri.split(/[#/]/);
    return parts[parts.length - 1] || uri;
  };

  return (
    <div className="flex items-start gap-2 p-2 bg-muted/30 rounded hover:bg-muted/50 transition-colors">
      {property.property_type === 'object_property' ? (
        <LinkIcon className="h-3.5 w-3.5 text-green-600 flex-shrink-0 mt-0.5" />
      ) : (
        <Database className="h-3.5 w-3.5 text-purple-600 flex-shrink-0 mt-0.5" />
      )}
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium truncate">{property.label}</p>
        {property.range.length > 0 && (
          <p className="text-xs text-muted-foreground truncate">
            → {property.range.map(extractLocalName).join(', ')}
          </p>
        )}
      </div>
    </div>
  );
};
