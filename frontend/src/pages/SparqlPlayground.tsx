import React, { useState, useMemo, useEffect } from 'react';
import Editor from '@monaco-editor/react';
import { Card, CardContent, CardHeader, CardTitle, CardDescription } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Tabs, TabsContent, TabsList, TabsTrigger } from '@/components/ui/tabs';
import {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  Sheet,
  SheetContent,
  SheetDescription,
  SheetHeader,
  SheetTitle,
  SheetTrigger,
} from '@/components/ui/sheet';
import { ScrollArea } from '@/components/ui/scroll-area';
import { ToggleGroup, ToggleGroupItem } from '@/components/ui/toggle-group';
import {
  Play,
  Save,
  Clock,
  Code2,
  Table2,
  GraduationCap,
  BookmarkIcon,
  CheckCircle2,
  XCircle,
  AlertTriangle,
  Loader2,
  Zap,
  FileJson,
} from 'lucide-react';
import { motion } from 'framer-motion';
import { formatDistanceToNow } from 'date-fns';
import { TemplateParameterForm } from '@/components/sparql/TemplateParameterForm';
import { ResultsTable } from '@/components/sparql/ResultsTable';
import {
  useSparqlQuery,
  useSparqlTemplates,
  useSparqlValidation,
  useQueryMode,
  useQueryHistory,
  useTableDensity,
  useSaveQuery,
} from '@/hooks/useSparql';
import type { SparqlTemplate } from '@/api/types';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';

const DEFAULT_QUERY = `PREFIX gph: <http://graphica.io/ontology#>
PREFIX prov: <http://www.w3.org/ns/prov#>

SELECT ?entity ?attrName ?confidence ?timestamp
WHERE {
  ?entity gph:hasDerivedAttribute ?attr .
  ?attr gph:attributeName ?attrName ;
        gph:confidence ?confidence ;
        prov:generatedAtTime ?timestamp .
  FILTER (?confidence > 0.8)
}
ORDER BY DESC(?confidence)
LIMIT 10`;

export function SparqlPlayground() {
  // Mode and state
  const { mode, setMode } = useQueryMode();
  const { density, setDensity } = useTableDensity();
  const { history } = useQueryHistory();

  // Reactive dark mode detection
  const [isDark, setIsDark] = useState(
    () => document.documentElement.classList.contains('dark')
  );

  // Watch for theme changes
  useEffect(() => {
    const observer = new MutationObserver((mutations) => {
      mutations.forEach((mutation) => {
        if (mutation.attributeName === 'class') {
          setIsDark(document.documentElement.classList.contains('dark'));
        }
      });
    });

    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['class'],
    });

    return () => observer.disconnect();
  }, []);

  // Query state
  const [query, setQuery] = useState(DEFAULT_QUERY);
  const [results, setResults] = useState<any[]>([]);
  const [executionTime, setExecutionTime] = useState<number | null>(null);

  // Template state (for beginner mode)
  const [selectedTemplateId, setSelectedTemplateId] = useState<string | null>(null);

  // Save query dialog
  const [showSaveDialog, setShowSaveDialog] = useState(false);
  const [saveQueryName, setSaveQueryName] = useState('');

  // API hooks
  const executeMutation = useSparqlQuery();
  const { data: templates } = useSparqlTemplates();
  const { data: validation } = useSparqlValidation(query);
  const saveQueryMutation = useSaveQuery();

  // Group templates by category
  const templatesByCategory = useMemo(() => {
    if (!templates) return {};
    return templates.reduce((acc, template) => {
      if (!acc[template.category]) {
        acc[template.category] = [];
      }
      acc[template.category].push(template);
      return acc;
    }, {} as Record<string, SparqlTemplate[]>);
  }, [templates]);

  const selectedTemplate = useMemo(() => {
    if (!selectedTemplateId || !templates) return null;
    return templates.find(t => t.id === selectedTemplateId) || null;
  }, [selectedTemplateId, templates]);

  const handleExecute = async () => {
    try {
      const result = await executeMutation.mutateAsync(query);
      setResults(result.results);
      setExecutionTime(result.executionTime);
    } catch (error) {
      // Error handled by mutation
      setResults([]);
    }
  };

  const handleTemplateGenerate = (generatedQuery: string) => {
    setQuery(generatedQuery);
    // Auto-switch to expert mode to show generated query
    setMode('expert');
  };

  const handleTemplateSelect = (templateId: string) => {
    setSelectedTemplateId(templateId);
    const template = templates?.find(t => t.id === templateId);
    if (template && template.parameters.length === 0) {
      // No parameters, directly set query
      setQuery(template.sparql);
    }
  };

  const handleHistorySelect = (historyQuery: string) => {
    setQuery(historyQuery);
    setMode('expert');
  };

  const handleSaveQuery = async () => {
    if (!saveQueryName.trim()) {
      toast.error('Please enter a query name');
      return;
    }

    await saveQueryMutation.mutateAsync({
      name: saveQueryName,
      description: '',
      query,
      tags: [],
    });

    setShowSaveDialog(false);
    setSaveQueryName('');
  };

  const handleExport = (format: 'csv' | 'json') => {
    if (format === 'csv') {
      exportToCsv(results);
    } else {
      exportToJson(results);
    }
  };

  const handleEditorDidMount = (editor: any, monaco: any) => {
    // Configure SPARQL language if not already registered
    if (!monaco.languages.getLanguages().some((lang: any) => lang.id === 'sparql')) {
      monaco.languages.register({ id: 'sparql' });

      // Define SPARQL syntax highlighting
      monaco.languages.setMonarchTokensProvider('sparql', {
        keywords: [
          'SELECT', 'DISTINCT', 'WHERE', 'FILTER', 'OPTIONAL', 'UNION', 'ORDER', 'BY',
          'LIMIT', 'OFFSET', 'ASC', 'DESC', 'FROM', 'NAMED', 'PREFIX', 'BASE',
          'CONSTRUCT', 'DESCRIBE', 'ASK', 'GROUP', 'HAVING', 'BIND', 'VALUES',
          'INSERT', 'DELETE', 'DATA', 'WITH', 'USING', 'GRAPH', 'DEFAULT', 'ALL',
          'AS', 'IN', 'NOT', 'EXISTS', 'MINUS', 'SERVICE', 'SILENT'
        ],
        operators: ['=', '!=', '<', '>', '<=', '>=', '&&', '||', '!', '+', '-', '*', '/'],
        symbols: /[=><!~?:&|+\-*\/\^%]+/,

        tokenizer: {
          root: [
            [/[A-Z]+\b/, {
              cases: {
                '@keywords': 'keyword',
                '@default': 'identifier'
              }
            }],
            [/\?[a-zA-Z_][a-zA-Z0-9_]*/, 'variable'],
            [/<[^>]+>/, 'type'],
            [/[a-zA-Z_][a-zA-Z0-9_]*:[a-zA-Z_][a-zA-Z0-9_]*/, 'type.identifier'],
            [/"([^"\\]|\\.)*$/, 'string.invalid'],
            [/'([^'\\]|\\.)*$/, 'string.invalid'],
            [/"/, 'string', '@string_double'],
            [/'/, 'string', '@string_single'],
            [/\d+\.\d+/, 'number.float'],
            [/\d+/, 'number'],
            [/#.*$/, 'comment'],
            [/@symbols/, 'operator'],
            [/\s+/, 'white'],
          ],
          string_double: [
            [/[^\\"]+/, 'string'],
            [/"/, 'string', '@pop']
          ],
          string_single: [
            [/[^\\']+/, 'string'],
            [/'/, 'string', '@pop']
          ],
        }
      });
    }

    // Keyboard shortcuts
    editor.addAction({
      id: 'execute-query',
      label: 'Execute Query',
      keybindings: [monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter],
      run: handleExecute,
    });
  };

  return (
    <div className="space-y-4 pb-8">
      {/* Header with Mode Switcher */}
      <motion.div
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.15 }}
        className="pb-4 border-b-2 border-border"
      >
        <div className="flex items-start justify-between mb-3">
          <div>
            <h1 className="text-2xl font-semibold text-foreground mb-1">
              SPARQL Playground
            </h1>
            <p className="text-sm text-muted-foreground">
              Query the RDF knowledge graph with SPARQL
            </p>
          </div>
        </div>

        {/* Command Bar */}
        <div className="flex items-center justify-between gap-4">
          {/* Mode Switcher */}
          <ToggleGroup type="single" value={mode} onValueChange={(val: any) => val && setMode(val)}>
            <ToggleGroupItem value="beginner" className="gap-2">
              <GraduationCap className="h-4 w-4" />
              Beginner
            </ToggleGroupItem>
            <ToggleGroupItem value="expert" className="gap-2">
              <Code2 className="h-4 w-4" />
              Expert
            </ToggleGroupItem>
          </ToggleGroup>

          {/* Actions */}
          <div className="flex items-center gap-2">
            {mode === 'beginner' && (
              <Select value={selectedTemplateId || ''} onValueChange={handleTemplateSelect}>
                <SelectTrigger className="w-[240px]">
                  <SelectValue placeholder="Select Template..." />
                </SelectTrigger>
                <SelectContent>
                  {Object.entries(templatesByCategory).map(([category, temps]) => (
                    <SelectGroup key={category}>
                      <SelectLabel>{category}</SelectLabel>
                      {temps.map(template => (
                        <SelectItem key={template.id} value={template.id}>
                          {template.name}
                        </SelectItem>
                      ))}
                    </SelectGroup>
                  ))}
                </SelectContent>
              </Select>
            )}

            <Button
              variant="default"
              className="gap-2"
              onClick={handleExecute}
              disabled={executeMutation.isPending || !validation?.valid}
            >
              {executeMutation.isPending ? (
                <>
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Executing...
                </>
              ) : (
                <>
                  <Play className="h-4 w-4" />
                  Execute
                </>
              )}
            </Button>

            <Button variant="outline" className="gap-2" onClick={() => setShowSaveDialog(true)}>
              <Save className="h-4 w-4" />
              Save
            </Button>

            {/* History Drawer */}
            <Sheet>
              <SheetTrigger asChild>
                <Button variant="outline" size="default" className="gap-2">
                  <Clock className="h-4 w-4" />
                  History
                </Button>
              </SheetTrigger>
              <SheetContent side="right" className="w-[400px]">
                <SheetHeader>
                  <SheetTitle>Query History</SheetTitle>
                  <SheetDescription>Recent SPARQL queries (last 50)</SheetDescription>
                </SheetHeader>
                <ScrollArea className="h-[calc(100vh-8rem)] mt-4">
                  <div className="space-y-2">
                    {history.length === 0 ? (
                      <p className="text-sm text-muted-foreground text-center py-8">
                        No query history yet
                      </p>
                    ) : (
                      history.map(item => (
                        <div
                          key={item.id}
                          className="p-3 rounded-md border border-border hover:bg-background-tertiary cursor-pointer transition-colors"
                          onClick={() => handleHistorySelect(item.query)}
                        >
                          <code className="text-xs text-muted-foreground block truncate">
                            {item.query.split('\n')[0]}...
                          </code>
                          <div className="flex items-center gap-3 mt-2 text-xs text-muted-foreground">
                            <span>{formatDistanceToNow(new Date(item.timestamp), { addSuffix: true })}</span>
                            {item.success ? (
                              <Badge variant="success" className="text-xs">
                                {item.results_count} results
                              </Badge>
                            ) : (
                              <Badge variant="destructive" className="text-xs">
                                Failed
                              </Badge>
                            )}
                            <span>{item.execution_time_ms.toFixed(0)}ms</span>
                          </div>
                        </div>
                      ))
                    )}
                  </div>
                </ScrollArea>
              </SheetContent>
            </Sheet>
          </div>
        </div>
      </motion.div>

      {/* Main Content Area */}
      <div className="grid gap-4 lg:grid-cols-5">
        {/* Left Pane - Query Input */}
        <motion.div
          initial={{ opacity: 0, x: -8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.15, delay: 0.05 }}
          className="lg:col-span-2 flex flex-col gap-4"
        >
          {mode === 'beginner' && selectedTemplate && (
            <TemplateParameterForm
              template={selectedTemplate}
              onGenerate={handleTemplateGenerate}
            />
          )}

          {mode === 'beginner' && !selectedTemplate && (
            <Card className="glass-morphism border-border">
              <CardContent className="p-12 text-center">
                <GraduationCap className="h-16 w-16 mx-auto mb-4 text-muted-foreground opacity-50" />
                <h3 className="text-lg font-semibold mb-2">Select a Template</h3>
                <p className="text-sm text-muted-foreground">
                  Choose a query template from the dropdown above to get started
                </p>
              </CardContent>
            </Card>
          )}

          {mode === 'expert' && (
            <Card className="glass-morphism border-border flex-1 flex flex-col">
              <CardHeader className="pb-3">
                <div className="flex items-center justify-between">
                  <CardTitle className="text-base">Query Editor</CardTitle>
                  <Badge variant={validation?.valid ? 'success' : 'destructive'}>
                    {validation?.valid ? 'Valid' : 'Invalid'}
                  </Badge>
                </div>
                <CardDescription>Write SPARQL queries with syntax highlighting</CardDescription>
              </CardHeader>
              <CardContent className="flex-1 flex flex-col min-h-[400px]">
                <div className="flex-1 border border-border-emphasis rounded-md overflow-hidden bg-background shadow-inner">
                  <Editor
                    height="100%"
                    language="sparql"
                    value={query}
                    onChange={(value) => setQuery(value || '')}
                    onMount={handleEditorDidMount}
                    theme={isDark ? 'vs-dark' : 'vs'}
                    options={{
                      minimap: { enabled: false },
                      fontSize: 13,
                      lineNumbers: 'on',
                      lineNumbersMinChars: 3,
                      roundedSelection: false,
                      scrollBeyondLastLine: false,
                      readOnly: false,
                      automaticLayout: true,
                      tabSize: 2,
                      wordWrap: 'on',
                      fontFamily: '"Cascadia Code", "JetBrains Mono", Consolas, monospace',
                      fontLigatures: true,
                      padding: { top: 12, bottom: 12 },
                      renderLineHighlight: 'line',
                      lineHeight: 20,
                      cursorBlinking: 'smooth',
                      scrollbar: {
                        vertical: 'visible',
                        horizontal: 'visible',
                        useShadows: false,
                        verticalScrollbarSize: 12,
                        horizontalScrollbarSize: 12,
                      },
                    }}
                  />
                </div>
              </CardContent>
            </Card>
          )}

          {/* Validation Panel */}
          {validation && mode === 'expert' && (
            <Card className="glass-morphism border-border">
              <CardContent className="p-4">
                {validation.valid ? (
                  <div className="flex items-center gap-2 text-sm text-success">
                    <CheckCircle2 className="h-4 w-4" />
                    Query is valid
                  </div>
                ) : (
                  <div className="space-y-2">
                    {validation.errors.map((err, i) => (
                      <div key={i} className="flex items-start gap-2 text-sm text-error">
                        <XCircle className="h-4 w-4 mt-0.5 flex-shrink-0" />
                        <span>{err}</span>
                      </div>
                    ))}
                  </div>
                )}

                {validation.warnings.length > 0 && (
                  <div className="mt-3 space-y-1">
                    {validation.warnings.map((warn, i) => (
                      <div key={i} className="flex items-start gap-2 text-sm text-warning">
                        <AlertTriangle className="h-4 w-4 mt-0.5 flex-shrink-0" />
                        <span>{warn}</span>
                      </div>
                    ))}
                  </div>
                )}

                {executionTime !== null && (
                  <div className="mt-3 flex items-center gap-2 text-xs text-muted-foreground">
                    <Zap className="h-3 w-3" />
                    Last execution: {executionTime.toFixed(0)}ms
                  </div>
                )}
              </CardContent>
            </Card>
          )}
        </motion.div>

        {/* Right Pane - Results */}
        <motion.div
          initial={{ opacity: 0, x: 8 }}
          animate={{ opacity: 1, x: 0 }}
          transition={{ duration: 0.15, delay: 0.1 }}
          className="lg:col-span-3 flex flex-col"
        >
          <Card className="glass-morphism border-border flex-1 flex flex-col">
            <CardHeader className="pb-3">
              <div className="flex items-center justify-between">
                <CardTitle className="text-base">Results</CardTitle>
                {results.length > 0 && (
                  <div className="flex items-center gap-2">
                    <Badge variant="outline">
                      {results.length} {results.length === 1 ? 'result' : 'results'}
                    </Badge>
                    <ToggleGroup type="single" value={density} onValueChange={(val: any) => val && setDensity(val)}>
                      <ToggleGroupItem value="compact" className="text-xs h-7 px-2">
                        Compact
                      </ToggleGroupItem>
                      <ToggleGroupItem value="comfortable" className="text-xs h-7 px-2">
                        Comfortable
                      </ToggleGroupItem>
                    </ToggleGroup>
                  </div>
                )}
              </div>
              <CardDescription>Query results visualization</CardDescription>
            </CardHeader>
            <CardContent className="flex-1 flex flex-col min-h-[500px]">
              {results.length === 0 ? (
                <div className="flex-1 flex items-center justify-center text-center">
                  <div>
                    <Table2 className="h-16 w-16 mx-auto mb-4 text-muted-foreground opacity-50" />
                    <h3 className="text-lg font-semibold mb-2">No Results</h3>
                    <p className="text-sm text-muted-foreground">
                      Execute a query to see results here
                    </p>
                  </div>
                </div>
              ) : (
                <Tabs defaultValue="table" className="flex-1 flex flex-col">
                  <TabsList className="bg-background-secondary border border-border">
                    <TabsTrigger value="table" className="gap-2">
                      <Table2 className="h-4 w-4" />
                      Table
                    </TabsTrigger>
                    <TabsTrigger value="json" className="gap-2">
                      <FileJson className="h-4 w-4" />
                      JSON
                    </TabsTrigger>
                  </TabsList>

                  <TabsContent value="table" className="flex-1 mt-4">
                    <ResultsTable
                      data={results}
                      onExport={handleExport}
                      density={density}
                      className="h-full"
                    />
                  </TabsContent>

                  <TabsContent value="json" className="flex-1 mt-4">
                    <div className="h-full border border-border-emphasis rounded-md p-4 bg-background-secondary overflow-auto shadow-inner">
                      <pre className="text-xs font-mono text-foreground">
                        {JSON.stringify(results, null, 2)}
                      </pre>
                    </div>
                  </TabsContent>
                </Tabs>
              )}
            </CardContent>
          </Card>
        </motion.div>
      </div>

      {/* Save Query Dialog */}
      {showSaveDialog && (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50">
          <Card className="w-[400px]">
            <CardHeader>
              <CardTitle>Save Query</CardTitle>
              <CardDescription>Save this query for later use</CardDescription>
            </CardHeader>
            <CardContent className="space-y-4">
              <div>
                <label className="text-sm font-medium">Query Name</label>
                <input
                  type="text"
                  className="w-full mt-1 px-3 py-2 border border-border rounded-md"
                  value={saveQueryName}
                  onChange={(e) => setSaveQueryName(e.target.value)}
                  placeholder="e.g., High Confidence Entities"
                />
              </div>
              <div className="flex gap-2">
                <Button onClick={handleSaveQuery} className="flex-1">
                  Save
                </Button>
                <Button variant="outline" onClick={() => setShowSaveDialog(false)} className="flex-1">
                  Cancel
                </Button>
              </div>
            </CardContent>
          </Card>
        </div>
      )}
    </div>
  );
}

// ============================================================================
// Helper Functions
// ============================================================================

function exportToCsv(data: Record<string, any>[]) {
  if (data.length === 0) {
    toast.error('No data to export');
    return;
  }

  const columns = Object.keys(data[0]);
  const csvHeader = columns.join(',');
  const csvRows = data.map(row =>
    columns.map(col => {
      const val = row[col];
      // Escape quotes and wrap in quotes if contains comma
      const str = String(val);
      return str.includes(',') ? `"${str.replace(/"/g, '""')}"` : str;
    }).join(',')
  );

  const csv = [csvHeader, ...csvRows].join('\n');
  const blob = new Blob([csv], { type: 'text/csv' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `sparql-results-${Date.now()}.csv`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);

  toast.success('Results exported to CSV');
}

function exportToJson(data: Record<string, any>[]) {
  if (data.length === 0) {
    toast.error('No data to export');
    return;
  }

  const json = JSON.stringify(data, null, 2);
  const blob = new Blob([json], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `sparql-results-${Date.now()}.json`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);

  toast.success('Results exported to JSON');
}
