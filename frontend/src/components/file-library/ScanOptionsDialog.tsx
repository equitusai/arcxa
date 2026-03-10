/**
 * Scan Options Dialog
 * Configure scanning options including ontology mapping
 */

import { useState, useEffect } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { Input } from '@/components/ui/input';
import { Switch } from '@/components/ui/switch';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Loader2, ScanSearch, Link2, Database } from 'lucide-react';
import { useQuery } from '@tanstack/react-query';
import { listOntologies } from '@/api/ontology';
import type { FileScanParams } from '@/api/fileLibrary';

interface ScanOptionsDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onConfirm: (params: FileScanParams) => void;
  fileCount: number; // Number of files to scan
  defaultOntologyId?: string; // From folder settings
}

export function ScanOptionsDialog({
  open,
  onOpenChange,
  onConfirm,
  fileCount,
  defaultOntologyId,
}: ScanOptionsDialogProps) {
  const [enableOntology, setEnableOntology] = useState(!!defaultOntologyId);
  const [ontologyId, setOntologyId] = useState(defaultOntologyId || '');
  const [sampleRows, setSampleRows] = useState(1000);

  // Fetch available ontologies
  const { data: ontologies, isLoading: loadingOntologies } = useQuery({
    queryKey: ['ontologies', 'active'],
    queryFn: () => listOntologies(true), // Only active ontologies
  });

  // Reset state when dialog opens
  useEffect(() => {
    if (open) {
      setEnableOntology(!!defaultOntologyId);
      setOntologyId(defaultOntologyId || '');
      setSampleRows(1000);
    }
  }, [open, defaultOntologyId]);

  // Auto-select first ontology if enabled but none selected
  useEffect(() => {
    if (enableOntology && !ontologyId && ontologies && ontologies.length > 0) {
      setOntologyId(ontologies[0].id);
    }
  }, [enableOntology, ontologyId, ontologies]);

  const handleConfirm = () => {
    const params: FileScanParams = {
      sample_rows: sampleRows,
      map_to_ontology: enableOntology,
      ontology_id: enableOntology ? ontologyId : undefined,
    };

    onConfirm(params);
    onOpenChange(false);
  };

  const hasOntologies = ontologies && ontologies.length > 0;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <ScanSearch className="h-5 w-5 text-primary" />
            Scan {fileCount === 1 ? 'File' : `${fileCount} Files`}
          </DialogTitle>
          <DialogDescription>
            Configure schema detection and ontology mapping options
          </DialogDescription>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* Sample Rows */}
          <div className="space-y-2">
            <Label htmlFor="sample-rows" className="text-sm font-medium">
              Sample Rows
            </Label>
            <Input
              id="sample-rows"
              type="number"
              min="100"
              max="100000"
              step="100"
              value={sampleRows}
              onChange={(e) => setSampleRows(parseInt(e.target.value) || 1000)}
              className="text-sm"
            />
            <p className="text-xs text-muted-foreground">
              Number of rows to analyze for schema inference
            </p>
          </div>

          {/* Ontology Mapping Toggle */}
          <div className="flex items-start justify-between gap-4 p-3 border rounded-lg">
            <div className="space-y-0.5 flex-1">
              <div className="flex items-center gap-2">
                <Link2 className="h-4 w-4 text-blue-600" />
                <Label htmlFor="enable-ontology" className="text-sm font-medium cursor-pointer">
                  Enable Ontology Mapping
                </Label>
              </div>
              <p className="text-xs text-muted-foreground">
                Automatically map detected fields to ontology concepts
              </p>
            </div>
            <Switch
              id="enable-ontology"
              checked={enableOntology}
              onCheckedChange={setEnableOntology}
            />
          </div>

          {/* Ontology Selection */}
          {enableOntology && (
            <div className="space-y-2 pl-3 border-l-2 border-blue-200">
              <Label htmlFor="ontology-select" className="text-sm font-medium">
                Select Ontology
              </Label>
              {loadingOntologies ? (
                <div className="flex items-center gap-2 p-3 text-sm text-muted-foreground">
                  <Loader2 className="h-4 w-4 animate-spin" />
                  Loading ontologies...
                </div>
              ) : !hasOntologies ? (
                <div className="p-3 bg-amber-50 border border-amber-200 rounded text-sm text-amber-800">
                  <div className="flex items-start gap-2">
                    <Database className="h-4 w-4 mt-0.5 flex-shrink-0" />
                    <div>
                      <div className="font-medium">No ontologies available</div>
                      <div className="text-xs mt-1">
                        Register an ontology first to enable semantic mapping
                      </div>
                    </div>
                  </div>
                </div>
              ) : (
                <>
                  <Select value={ontologyId} onValueChange={setOntologyId}>
                    <SelectTrigger id="ontology-select">
                      <SelectValue placeholder="Choose an ontology..." />
                    </SelectTrigger>
                    <SelectContent>
                      {ontologies.map((ontology) => (
                        <SelectItem key={ontology.id} value={ontology.id}>
                          <span className="font-medium">
                            {ontology.name || ontology.id}
                          </span>
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                  {ontologyId && defaultOntologyId === ontologyId && (
                    <p className="text-xs text-blue-600 flex items-center gap-1">
                      <Database className="h-3 w-3" />
                      Using folder default ontology
                    </p>
                  )}
                </>
              )}
            </div>
          )}

          {/* Preview */}
          <div className="p-3 bg-muted rounded-lg text-xs space-y-1.5">
            <div className="font-medium text-foreground">Scan Configuration:</div>
            <div className="text-muted-foreground">
              • Schema inference: <span className="text-foreground">Enabled</span>
            </div>
            <div className="text-muted-foreground">
              • Sample size: <span className="text-foreground">{sampleRows.toLocaleString()} rows</span>
            </div>
            <div className="text-muted-foreground">
              • Ontology mapping:{' '}
              <span className="text-foreground">
                {enableOntology && ontologyId ? (
                  <>
                    {ontologies?.find((o) => o.id === ontologyId)?.name || ontologyId}
                  </>
                ) : (
                  'Disabled'
                )}
              </span>
            </div>
            <div className="text-muted-foreground">
              • Auto-save: <span className="text-foreground">Enabled</span>
            </div>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            Cancel
          </Button>
          <Button onClick={handleConfirm} disabled={enableOntology && !ontologyId}>
            <ScanSearch className="h-4 w-4 mr-2" />
            Scan {fileCount === 1 ? 'File' : 'Files'}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
