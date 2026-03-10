/**
 * Ontologies Management Page
 *
 * Allows users to:
 * - View all registered ontologies
 * - Register new custom ontologies
 * - View ontology tree structure
 * - Activate/deactivate ontologies
 * - Delete ontologies
 */

import React, { useState, useEffect } from 'react';
import {
  Plus,
  Database,
  Trash2,
  Eye,
  EyeOff,
  Download,
  Upload,
  ChevronLeft,
  ChevronRight,
  BarChart3,
  Network,
  FileJson,
  Image as ImageIcon,
  Grid3x3,
  TreePine,
  Repeat,
  FileCode,
  AlertTriangle,
  Pencil,
} from 'lucide-react';
import {
  OntologyMetadata,
  OntologyTreeResponse,
  RegisterOntologyRequest,
  RegisteredOntology,
  ClassNode,
  PropertyNode,
} from '../api/ontology';
import {
  useOntologies,
  useOntology,
  useOntologyTree,
  useRegisterOntology,
  useUpdateOntology,
  useActivateOntology,
  useDeactivateOntology,
  useDeleteOntology,
} from '../hooks/useOntologies';
import { EnhancedTreeView } from '../components/ontology/EnhancedTreeView';
import { InspectorPane } from '../components/ontology/InspectorPane';
import { GlobalSearch } from '../components/ontology/GlobalSearch';
import { StatsDashboard } from '../components/ontology/StatsDashboard';
import { SPARQLEditor } from '../components/ontology/SPARQLEditor';
import { Button } from '../components/ui/button';
import {
  detectRDFFormat,
  convertToTurtle,
  getFormatDisplayName,
  type RDFFormat,
} from '../lib/rdfConverter';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '../components/ui/dropdown-menu';
import { cn } from '../lib/utils';

export const Ontologies: React.FC = () => {
  // React Query hooks for data fetching
  const { data: ontologies = [], isLoading: loading, error: queryError } = useOntologies(false);
  const [selectedOntology, setSelectedOntology] = useState<string | null>(null);
  const { data: treeData, isLoading: loadingTree } = useOntologyTree(selectedOntology || undefined, {
    maxDepth: -1,
    includeProperties: true,
    includeIndividuals: false,
  });

  // Mutation hooks
  const activateMutation = useActivateOntology();
  const deactivateMutation = useDeactivateOntology();
  const deleteMutation = useDeleteOntology();

  // Local UI state
  const [showRegisterDialog, setShowRegisterDialog] = useState(false);
  const [editingOntology, setEditingOntology] = useState<OntologyMetadata | null>(null);
  const error = queryError ? (queryError as Error).message : null;

  // Inspector pane state
  const [selectedNode, setSelectedNode] = useState<ClassNode | PropertyNode | null>(null);
  const [selectedNodeType, setSelectedNodeType] = useState<'class' | 'property' | null>(null);

  // Week 1: Three-zone shell state
  const [railCollapsed, setRailCollapsed] = useState(false);
  const [viewMode, setViewMode] = useState<'grid' | 'tree'>('tree');
  const [showExportMenu, setShowExportMenu] = useState(false);

  // Week 1.5: Export handlers
  const handleExport = (format: 'turtle' | 'jsonld' | 'png' | 'csv') => {
    if (!treeData || !selectedOntology) return;

    switch (format) {
      case 'turtle':
        // Export as Turtle RDF format
        const turtle = `# Ontology: ${selectedOntology}\n# Namespace: ${treeData.namespace}\n\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\n# Classes: ${treeData.stats.total_classes}\n# Properties: ${treeData.stats.total_properties}\n`;
        downloadText(turtle, `${selectedOntology}.ttl`, 'text/turtle');
        break;
      case 'jsonld':
        // Export as JSON-LD
        const jsonld = {
          '@context': {
            '@vocab': treeData.namespace,
            owl: 'http://www.w3.org/2002/07/owl#',
            rdfs: 'http://www.w3.org/2000/01/rdf-schema#',
          },
          '@type': 'owl:Ontology',
          'rdfs:label': selectedOntology,
          classes: treeData.root_classes,
          properties: treeData.root_properties,
          stats: treeData.stats,
        };
        downloadText(JSON.stringify(jsonld, null, 2), `${selectedOntology}.jsonld`, 'application/ld+json');
        break;
      case 'csv':
        // Export classes as CSV
        let csv = 'URI,Label,Comment,Subclasses,Properties\n';
        const flattenClasses = (classes: ClassNode[], parent = ''): string => {
          return classes
            .map((cls) => {
              const row = [
                cls.uri,
                cls.label || '',
                cls.comment || '',
                (cls.subclasses?.length || 0).toString(),
                (cls.properties?.length || 0).toString(),
              ]
                .map((v) => `"${v.replace(/"/g, '""')}"`)
                .join(',');
              const subrows = cls.subclasses ? flattenClasses(cls.subclasses, cls.uri) : '';
              return row + '\n' + subrows;
            })
            .join('');
        };
        csv += flattenClasses(treeData.root_classes);
        downloadText(csv, `${selectedOntology}.csv`, 'text/csv');
        break;
      case 'png':
        // Note: PNG export would require canvas rendering - show placeholder toast
        alert('PNG export requires visualization rendering. Feature coming soon!');
        break;
    }
    setShowExportMenu(false);
  };

  const downloadText = (content: string, filename: string, mimeType: string) => {
    const blob = new Blob([content], { type: mimeType });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = filename;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  // Handlers using React Query mutations
  const handleToggleActive = (id: string, currentlyActive: boolean) => {
    if (currentlyActive) {
      deactivateMutation.mutate(id);
    } else {
      activateMutation.mutate(id);
    }
  };

  const handleDelete = (id: string, permanent: boolean = false) => {
    const message = permanent
      ? 'Are you sure you want to permanently delete this ontology? This cannot be undone.'
      : 'Are you sure you want to deactivate this ontology?';

    if (!confirm(message)) {
      return;
    }

    deleteMutation.mutate(
      { id, permanent },
      {
        onSuccess: () => {
          // Clear selection if deleted ontology was selected
          if (selectedOntology === id) {
            setSelectedOntology(null);
          }
        },
      }
    );
  };

  // Find node by URI in the tree
  const findNodeByUri = (uri: string, type: 'class' | 'property'): ClassNode | PropertyNode | null => {
    if (!treeData) return null;

    if (type === 'class') {
      const searchInClasses = (classes: ClassNode[]): ClassNode | null => {
        for (const cls of classes) {
          if (cls.uri === uri) return cls;
          if (cls.subclasses) {
            const found = searchInClasses(cls.subclasses);
            if (found) return found;
          }
        }
        return null;
      };
      return searchInClasses(treeData.root_classes);
    } else {
      // Search in root properties
      for (const prop of treeData.root_properties) {
        if (prop.uri === uri) return prop;
      }
      // Also search in class properties
      const searchPropsInClasses = (classes: ClassNode[]): PropertyNode | null => {
        for (const cls of classes) {
          if (cls.properties) {
            for (const prop of cls.properties) {
              if (prop.uri === uri) return prop;
            }
          }
          if (cls.subclasses) {
            const found = searchPropsInClasses(cls.subclasses);
            if (found) return found;
          }
        }
        return null;
      };
      return searchPropsInClasses(treeData.root_classes);
    }
  };

  const handleNodeClick = (uri: string, type: 'class' | 'property', node?: ClassNode | PropertyNode) => {
    const foundNode = node || findNodeByUri(uri, type);
    if (foundNode) {
      setSelectedNode(foundNode);
      setSelectedNodeType(type);
    }
  };

  const handleCloseInspector = () => {
    setSelectedNode(null);
    setSelectedNodeType(null);
  };

  if (loading) {
    return (
      <div className="container mx-auto p-6">
        <div className="flex items-start justify-between mb-4">
          <div className="flex-1">
            <h1 className="text-3xl font-bold">Ontologies</h1>
            <p className="text-muted-foreground mt-1">
              Manage custom domain ontologies for intelligent field mapping
            </p>
          </div>
        </div>

        {/* Skeleton for search bar */}
        <div className="mb-6 max-w-2xl">
          <div className="h-10 bg-muted/30 rounded-lg animate-pulse" />
        </div>

        <div className="grid grid-cols-1 lg:grid-cols-2 gap-6">
          {/* Skeleton for ontology list */}
          <div className="space-y-4">
            <div className="flex items-center justify-between">
              <h2 className="text-lg font-semibold">Registered Ontologies</h2>
            </div>
            <div className="space-y-2">
              {[1, 2, 3].map((i) => (
                <div key={i} className="border rounded-lg p-3 animate-pulse">
                  <div className="flex items-center justify-between gap-3">
                    <div className="flex-1">
                      <div className="h-5 bg-muted rounded w-3/4 mb-2" />
                      <div className="h-3 bg-muted/60 rounded w-1/2" />
                    </div>
                    <div className="flex gap-1">
                      <div className="w-8 h-8 bg-muted rounded" />
                      <div className="w-8 h-8 bg-muted rounded" />
                    </div>
                  </div>
                </div>
              ))}
            </div>
          </div>

          {/* Skeleton for tree view */}
          <div className="border rounded-lg p-6 bg-card">
            <h2 className="text-lg font-semibold mb-4">Ontology Structure</h2>
            <div className="space-y-3 animate-pulse">
              <div className="h-4 bg-muted rounded w-full" />
              <div className="h-4 bg-muted/70 rounded w-5/6 ml-4" />
              <div className="h-4 bg-muted/50 rounded w-4/6 ml-8" />
              <div className="h-4 bg-muted rounded w-full" />
              <div className="h-4 bg-muted/70 rounded w-3/4 ml-4" />
            </div>
          </div>
        </div>
      </div>
    );
  }

  const selectedOntologyData = ontologies?.find((ont) => ont.id === selectedOntology);

  return (
    <>
      {/* Week 1: Three-Zone Enterprise Shell */}
      <div className="flex h-screen overflow-hidden bg-background">
        {/* Left Rail: Ontology List */}
        <div
          className={cn(
            'border-r bg-muted/30 flex flex-col transition-all duration-300',
            railCollapsed ? 'w-16' : 'w-64'
          )}
        >
          {/* Rail Header */}
          <div className="p-4 border-b bg-background flex items-center justify-between">
            {!railCollapsed && (
              <div className="flex-1 min-w-0">
                <h2 className="text-base font-semibold truncate">Ontologies</h2>
                <p className="text-xs text-muted-foreground">{ontologies.length} total</p>
              </div>
            )}
            <Button
              variant="ghost"
              size="sm"
              className="h-8 w-8 p-0 flex-shrink-0"
              onClick={() => setRailCollapsed(!railCollapsed)}
            >
              {railCollapsed ? (
                <ChevronRight className="h-4 w-4" />
              ) : (
                <ChevronLeft className="h-4 w-4" />
              )}
            </Button>
          </div>

          {/* Rail Content */}
          <div className="flex-1 overflow-y-auto p-2">
            {railCollapsed ? (
              /* Collapsed: Icon-only view */
              <div className="space-y-2">
                {ontologies.map((ontology) => (
                  <button
                    key={ontology.id}
                    className={cn(
                      'w-full h-12 flex items-center justify-center rounded-lg transition-all relative group',
                      selectedOntology === ontology.id
                        ? 'bg-primary text-primary-foreground'
                        : 'hover:bg-muted'
                    )}
                    onClick={() => setSelectedOntology(ontology.id)}
                    title={ontology.name || ontology.id}
                  >
                    <Database className="h-5 w-5" />
                    {ontology.active && (
                      <div className="absolute top-1 right-1 w-2 h-2 rounded-full bg-green-500" />
                    )}
                  </button>
                ))}
                <button
                  className="w-full h-12 flex items-center justify-center rounded-lg border-2 border-dashed hover:bg-muted transition-colors"
                  onClick={() => setShowRegisterDialog(true)}
                  title="Register Ontology"
                >
                  <Plus className="h-5 w-5" />
                </button>
              </div>
            ) : (
              /* Expanded: Full cards */
              <div className="space-y-2">
                {ontologies.length === 0 ? (
                  <div className="text-center py-8 px-2">
                    <Database className="h-10 w-10 text-muted-foreground mx-auto mb-3" />
                    <p className="text-sm font-medium mb-1">No Ontologies</p>
                    <p className="text-xs text-muted-foreground mb-3">
                      Register your first ontology
                    </p>
                    <Button size="sm" onClick={() => setShowRegisterDialog(true)}>
                      <Plus className="h-3 w-3 mr-1.5" />
                      Register
                    </Button>
                  </div>
                ) : (
                  <>
                    {ontologies.map((ontology) => (
                      <div
                        key={ontology.id}
                        className={cn(
                          'border-2 rounded-lg p-3 cursor-pointer transition-all',
                          selectedOntology === ontology.id
                            ? 'border-primary bg-primary/5 shadow-sm'
                            : 'border-transparent hover:bg-muted/70 hover:border-muted-foreground/20'
                        )}
                        onClick={() => setSelectedOntology(ontology.id)}
                      >
                        <div className="flex items-start justify-between gap-2">
                          <div className="flex-1 min-w-0">
                            <div className="flex items-center gap-2 mb-1">
                              <h3 className={cn(
                                "font-semibold text-sm truncate",
                                !ontology.active && "text-muted-foreground"
                              )}>
                                {ontology.name ||
                                  ontology.id
                                    .split('_')
                                    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
                                    .join(' ')}
                              </h3>
                              <div className={cn(
                                "w-2 h-2 rounded-full flex-shrink-0",
                                ontology.active ? "bg-green-500" : "bg-gray-400"
                              )}
                              title={ontology.active ? "Active" : "Inactive"}
                              />
                            </div>
                            {!ontology.name && (
                              <p className="text-xs text-muted-foreground truncate">
                                {ontology.id}
                              </p>
                            )}
                            {!ontology.active && (
                              <p className="text-xs text-amber-600 dark:text-amber-500">
                                Inactive
                              </p>
                            )}
                          </div>
                          <div className="flex gap-0.5 flex-shrink-0">
                            <button
                              className="p-1 hover:bg-background rounded transition-colors"
                              onClick={(e) => {
                                e.stopPropagation();
                                setEditingOntology(ontology);
                              }}
                              title="Edit"
                            >
                              <Pencil className="h-3.5 w-3.5" />
                            </button>
                            <button
                              className="p-1 hover:bg-background rounded transition-colors"
                              onClick={(e) => {
                                e.stopPropagation();
                                handleToggleActive(ontology.id, ontology.active);
                              }}
                              title={ontology.active ? 'Deactivate' : 'Activate'}
                            >
                              {ontology.active ? (
                                <Eye className="h-3.5 w-3.5" />
                              ) : (
                                <EyeOff className="h-3.5 w-3.5 text-muted-foreground" />
                              )}
                            </button>
                            <button
                              className="p-1 hover:bg-destructive/10 text-destructive rounded transition-colors"
                              onClick={(e) => {
                                e.stopPropagation();
                                handleDelete(ontology.id);
                              }}
                              title="Delete"
                            >
                              <Trash2 className="h-3.5 w-3.5" />
                            </button>
                          </div>
                        </div>
                      </div>
                    ))}
                    <Button
                      variant="outline"
                      size="sm"
                      className="w-full mt-2"
                      onClick={() => setShowRegisterDialog(true)}
                    >
                      <Plus className="h-3.5 w-3.5 mr-1.5" />
                      Register Ontology
                    </Button>
                  </>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Main Canvas */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {/* Command Bar */}
          <div className="border-b bg-background p-4 flex items-center justify-between gap-4">
            <div className="flex-1 min-w-0">
              <h1 className="text-2xl font-bold truncate">
                {selectedOntologyData?.name ||
                  selectedOntology?.split('_').map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(' ') ||
                  'Ontologies'}
              </h1>
              <p className="text-sm text-muted-foreground">
                {selectedOntology
                  ? 'Explore classes, properties, and relationships'
                  : 'Manage custom domain ontologies for intelligent field mapping'}
              </p>
            </div>

            {selectedOntology && treeData && (
              <div className="flex items-center gap-2">
                {/* View Mode Toggle */}
                <div className="flex items-center gap-1 border rounded-md p-1">
                  <Button
                    variant={viewMode === 'tree' ? 'default' : 'ghost'}
                    size="sm"
                    className="h-7 px-2"
                    onClick={() => setViewMode('tree')}
                  >
                    <TreePine className="h-3.5 w-3.5" />
                  </Button>
                  <Button
                    variant={viewMode === 'grid' ? 'default' : 'ghost'}
                    size="sm"
                    className="h-7 px-2"
                    onClick={() => setViewMode('grid')}
                  >
                    <Grid3x3 className="h-3.5 w-3.5" />
                  </Button>
                </div>

                {/* Week 1.5: Export Dropdown */}
                <DropdownMenu>
                  <DropdownMenuTrigger asChild>
                    <Button variant="outline" size="sm">
                      <Download className="h-3.5 w-3.5 mr-1.5" />
                      Export
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align="end">
                    <DropdownMenuItem onClick={() => handleExport('turtle')}>
                      <FileJson className="h-3.5 w-3.5 mr-2" />
                      Turtle (.ttl)
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => handleExport('jsonld')}>
                      <FileJson className="h-3.5 w-3.5 mr-2" />
                      JSON-LD (.jsonld)
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => handleExport('csv')}>
                      <FileJson className="h-3.5 w-3.5 mr-2" />
                      CSV (.csv)
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => handleExport('png')}>
                      <ImageIcon className="h-3.5 w-3.5 mr-2" />
                      PNG Image (.png)
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            )}
          </div>

          {/* Error Banner */}
          {error && (
            <div className="bg-destructive/10 border-b border-destructive p-4">
              <p className="text-destructive text-sm">{error}</p>
            </div>
          )}

          {/* Main Content Area */}
          <div className="flex-1 overflow-y-auto p-6">
            {!selectedOntology ? (
              /* Empty State */
              <div className="flex items-center justify-center h-full">
                <div className="text-center max-w-md">
                  <Database className="h-16 w-16 text-muted-foreground mx-auto mb-4 opacity-50" />
                  <h2 className="text-xl font-semibold mb-2">No Ontology Selected</h2>
                  <p className="text-muted-foreground mb-6">
                    Select an ontology from the left rail to view its structure, or register a new
                    ontology to get started.
                  </p>
                  <Button onClick={() => setShowRegisterDialog(true)}>
                    <Plus className="h-4 w-4 mr-2" />
                    Register Ontology
                  </Button>
                </div>
              </div>
            ) : loadingTree ? (
              /* Loading Skeleton */
              <div className="space-y-6 animate-pulse">
                <div className="grid grid-cols-2 gap-4">
                  {[1, 2, 3, 4].map((i) => (
                    <div key={i} className="border-2 rounded-lg p-6 bg-muted/30">
                      <div className="h-10 bg-muted rounded mb-3" />
                      <div className="h-8 bg-muted/70 rounded" />
                    </div>
                  ))}
                </div>
                <div className="h-64 bg-muted/30 rounded-lg" />
              </div>
            ) : treeData ? (
              /* Content with Stats Dashboard + Tree */
              <div className="space-y-6">
                {/* Week 1.3: Visual Stats Dashboard */}
                <StatsDashboard
                  treeData={treeData}
                  ontologyName={
                    selectedOntologyData?.name ||
                    selectedOntology.split('_').map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(' ')
                  }
                />

                {/* Global Search */}
                <div className="max-w-2xl">
                  <GlobalSearch
                    rootClasses={treeData.root_classes}
                    rootProperties={treeData.root_properties}
                    onSelectResult={handleNodeClick}
                  />
                </div>

                {/* Tree View */}
                <div className="border-2 rounded-lg p-6 bg-card">
                  <h3 className="text-lg font-semibold mb-4">Ontology Structure</h3>
                  <div className="max-h-[600px] overflow-y-auto pr-2">
                    <EnhancedTreeView
                      rootClasses={treeData.root_classes}
                      rootProperties={treeData.root_properties}
                      onNodeClick={handleNodeClick}
                    />
                  </div>
                </div>
              </div>
            ) : (
              /* Error State */
              <div className="flex items-center justify-center h-full">
                <div className="text-center">
                  <p className="text-destructive">Failed to load ontology tree structure</p>
                </div>
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Register/Edit Ontology Dialog */}
      {(showRegisterDialog || editingOntology) && (
        <RegisterOntologyDialog
          onClose={() => {
            setShowRegisterDialog(false);
            setEditingOntology(null);
          }}
          onSuccess={() => {
            setShowRegisterDialog(false);
            setEditingOntology(null);
            // React Query hook automatically refetches ontologies list
          }}
          editingOntology={editingOntology}
        />
      )}

      {/* Inspector Pane */}
      {selectedNode && (
        <InspectorPane
          selectedNode={selectedNode}
          nodeType={selectedNodeType}
          onClose={handleCloseInspector}
        />
      )}
    </>
  );
};

/**
 * Dialog for registering or editing an ontology
 */
interface RegisterOntologyDialogProps {
  onClose: () => void;
  onSuccess: () => void;
  editingOntology?: OntologyMetadata | null;
}

const RegisterOntologyDialog: React.FC<RegisterOntologyDialogProps> = ({
  onClose,
  onSuccess,
  editingOntology,
}) => {
  const [formData, setFormData] = useState<RegisterOntologyRequest>({
    id: '',
    name: '',
    description: '',
    namespace: '',
    content: '',
    tags: [],
    version: '1.0.0',
  });
  const [detectedFormat, setDetectedFormat] = useState<RDFFormat>('unknown');
  const [converting, setConverting] = useState(false);
  const [conversionError, setConversionError] = useState<string | null>(null);

  const isEditMode = !!editingOntology;

  // React Query hooks
  const { data: fullOntology, isLoading: loadingOntology, error: loadError } = useOntology(editingOntology?.id);
  const registerMutation = useRegisterOntology();
  const updateMutation = useUpdateOntology();

  const submitting = registerMutation.isPending || updateMutation.isPending;
  const error = loadError ? (loadError as Error).message :
                (registerMutation.error ? (registerMutation.error as Error).message :
                (updateMutation.error ? (updateMutation.error as Error).message :
                conversionError));

  // Load existing ontology data when in edit mode
  useEffect(() => {
    if (!editingOntology) {
      // Reset form for new registration
      setFormData({
        id: '',
        name: '',
        description: '',
        namespace: '',
        content: '',
        tags: [],
        version: '1.0.0',
      });
      return;
    }

    if (fullOntology) {
      setFormData({
        id: fullOntology.metadata.id,
        name: fullOntology.metadata.name || '',
        description: fullOntology.metadata.description || '',
        namespace: fullOntology.metadata.namespace,
        content: fullOntology.content || '',
        tags: fullOntology.metadata.tags || [],
        version: fullOntology.metadata.version || '1.0.0',
      });
    }
  }, [editingOntology, fullOntology]);

  // Detect format when content changes
  useEffect(() => {
    if (formData.content.trim()) {
      const format = detectRDFFormat(formData.content);
      setDetectedFormat(format);
    } else {
      setDetectedFormat('unknown');
    }
  }, [formData.content]);

  const handleConvertToTurtle = async () => {
    setConverting(true);
    setConversionError(null);

    try {
      const result = await convertToTurtle(formData.content, detectedFormat);

      if (result.success) {
        setFormData({ ...formData, content: result.turtle });
        setDetectedFormat('turtle');
      } else {
        setConversionError(`Conversion failed: ${result.error}`);
      }
    } catch (err: any) {
      setConversionError(`Conversion error: ${err.message}`);
    } finally {
      setConverting(false);
    }
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();

    if (isEditMode) {
      // Update existing ontology
      updateMutation.mutate(
        { id: formData.id, updates: formData },
        {
          onSuccess: () => {
            onSuccess();
          },
        }
      );
    } else {
      // Register new ontology
      registerMutation.mutate(formData, {
        onSuccess: () => {
          onSuccess();
        },
      });
    }
  };

  return (
    <div className="fixed inset-0 bg-background/80 backdrop-blur-sm z-50 flex items-center justify-center p-4">
      <div className="bg-card border rounded-lg shadow-lg max-w-2xl w-full max-h-[90vh] overflow-y-auto">
        <div className="p-6">
          <h2 className="text-2xl font-bold mb-4">
            {isEditMode ? 'Edit Ontology' : 'Register Custom Ontology'}
          </h2>

          {loadingOntology && (
            <div className="bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-900 rounded-lg p-4 mb-4">
              <div className="flex items-center gap-2">
                <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-blue-600"></div>
                <p className="text-blue-600 dark:text-blue-400 text-sm">Loading ontology data...</p>
              </div>
            </div>
          )}

          {error && (
            <div className="bg-destructive/10 border border-destructive rounded-lg p-4 mb-4">
              <p className="text-destructive text-sm">{error}</p>
            </div>
          )}

          <form onSubmit={handleSubmit} className="space-y-4">
            <div>
              <label className="block text-sm font-medium mb-1">
                Ontology ID <span className="text-destructive">*</span>
              </label>
              <input
                type="text"
                className="w-full px-3 py-2 border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary disabled:bg-muted disabled:cursor-not-allowed"
                value={formData.id}
                onChange={(e) => setFormData({ ...formData, id: e.target.value })}
                placeholder="e.g., retail_domain"
                required
                disabled={isEditMode} // Can't change ID when editing
              />
              {isEditMode && (
                <p className="text-xs text-muted-foreground mt-1">
                  ID cannot be changed when editing
                </p>
              )}
            </div>

            <div>
              <label className="block text-sm font-medium mb-1">
                Name <span className="text-destructive">*</span>
              </label>
              <input
                type="text"
                className="w-full px-3 py-2 border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary"
                value={formData.name}
                onChange={(e) => setFormData({ ...formData, name: e.target.value })}
                placeholder="e.g., Retail Domain Ontology"
                required
              />
            </div>

            <div>
              <label className="block text-sm font-medium mb-1">Description</label>
              <textarea
                className="w-full px-3 py-2 border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary"
                value={formData.description}
                onChange={(e) => setFormData({ ...formData, description: e.target.value })}
                placeholder="Describe your ontology..."
                rows={2}
              />
            </div>

            <div>
              <label className="block text-sm font-medium mb-1">
                Namespace <span className="text-destructive">*</span>
              </label>
              <input
                type="text"
                className="w-full px-3 py-2 border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary font-mono text-sm"
                value={formData.namespace}
                onChange={(e) => setFormData({ ...formData, namespace: e.target.value })}
                placeholder="http://example.com/ontology#"
                required
              />
            </div>

            <div>
              <div className="flex items-center justify-between mb-2">
                <label className="block text-sm font-medium">
                  Ontology Content <span className="text-destructive">*</span>
                </label>
                {detectedFormat !== 'unknown' && (
                  <div className="flex items-center gap-2">
                    <div className={`flex items-center gap-1.5 px-2 py-1 rounded-md text-xs font-medium ${
                      detectedFormat === 'turtle'
                        ? 'bg-green-100 dark:bg-green-950 text-green-700 dark:text-green-400'
                        : 'bg-blue-100 dark:bg-blue-950 text-blue-700 dark:text-blue-400'
                    }`}>
                      <FileCode className="h-3.5 w-3.5" />
                      {getFormatDisplayName(detectedFormat)}
                    </div>
                    {(detectedFormat === 'rdfxml' || detectedFormat === 'ntriples' || detectedFormat === 'nquads') && (
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={handleConvertToTurtle}
                        disabled={converting}
                        className="h-7 text-xs"
                        title="Convert to Turtle format for better readability and validation"
                      >
                        {converting ? (
                          <>
                            <div className="animate-spin rounded-full h-3 w-3 border-b-2 border-current mr-1.5"></div>
                            Converting...
                          </>
                        ) : (
                          <>
                            <Repeat className="h-3.5 w-3.5 mr-1.5" />
                            Convert to Turtle
                          </>
                        )}
                      </Button>
                    )}
                  </div>
                )}
              </div>

              {detectedFormat === 'rdfxml' && (
                <div className="mb-3 p-3 bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-900 rounded-lg">
                  <div className="flex items-start gap-2">
                    <FileCode className="h-4 w-4 text-blue-600 dark:text-blue-500 mt-0.5 flex-shrink-0" />
                    <div className="text-sm">
                      <p className="font-medium text-blue-900 dark:text-blue-300">
                        RDF/XML format detected
                      </p>
                      <p className="text-blue-700 dark:text-blue-400 text-xs mt-1">
                        Backend accepts RDF/XML directly. Optionally convert to Turtle for better validation and readability.
                      </p>
                    </div>
                  </div>
                </div>
              )}

              <SPARQLEditor
                value={formData.content}
                onChange={(value) => setFormData({ ...formData, content: value })}
                height={350}
                language="turtle"
                placeholder="Paste Turtle or RDF/XML content here...&#10;&#10;@prefix ex: <http://example.com/ontology#> .&#10;ex:MyClass a owl:Class ."
                showValidation={detectedFormat === 'turtle'}
              />
              <p className="text-xs text-muted-foreground mt-2">
                Supports: <strong>Turtle</strong>, <strong>RDF/XML</strong>, <strong>N-Triples</strong>, <strong>N-Quads</strong>.
                All formats are accepted by the backend. Convert to Turtle for validation and readability.
              </p>
            </div>

            <div>
              <label className="block text-sm font-medium mb-1">Tags (comma-separated)</label>
              <input
                type="text"
                className="w-full px-3 py-2 border rounded-lg focus:outline-none focus:ring-2 focus:ring-primary"
                value={formData.tags?.join(', ')}
                onChange={(e) =>
                  setFormData({
                    ...formData,
                    tags: e.target.value.split(',').map((t) => t.trim()).filter((t) => t),
                  })
                }
                placeholder="e.g., retail, e-commerce, products"
              />
            </div>

            <div className="flex gap-3 pt-4">
              <Button
                type="submit"
                disabled={submitting || loadingOntology}
                className="flex-1"
              >
                {submitting ? (
                  <>
                    <div className="animate-spin rounded-full h-4 w-4 border-b-2 border-white mr-2"></div>
                    {isEditMode ? 'Updating...' : 'Registering...'}
                  </>
                ) : (
                  <>
                    {isEditMode ? (
                      <>
                        <Pencil className="h-4 w-4 mr-2" />
                        Update Ontology
                      </>
                    ) : (
                      <>
                        <Upload className="h-4 w-4 mr-2" />
                        Register Ontology
                      </>
                    )}
                  </>
                )}
              </Button>
              <Button type="button" variant="outline" onClick={onClose} disabled={submitting}>
                Cancel
              </Button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
};
