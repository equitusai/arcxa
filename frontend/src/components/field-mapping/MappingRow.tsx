/**
 * MappingRow Component
 *
 * Individual row in the field mapping table showing a single field
 * with its AI suggestions and user action buttons.
 */

import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { Textarea } from '@/components/ui/textarea';
import {
  Tooltip,
  TooltipContent,
  TooltipProvider,
  TooltipTrigger,
} from '@/components/ui/tooltip';
import { ConfidenceBadge } from './ConfidenceBadge';
import { Check, X, Edit2, ChevronDown, ChevronUp, Info } from 'lucide-react';
import type { FieldMapping, MappingAction } from '@/api/field-mapping';

interface MappingRowProps {
  mapping: FieldMapping;
  onActionChange: (fieldId: string, action: MappingAction, selectedUri?: string, notes?: string) => void;
  defaultExpanded?: boolean;
}

export function MappingRow({
  mapping,
  onActionChange,
  defaultExpanded = false,
}: MappingRowProps) {
  const [isExpanded, setIsExpanded] = useState(defaultExpanded);
  const [selectedUri, setSelectedUri] = useState(
    mapping.selected_mapping?.ontology_term_uri ||
    mapping.candidates[0]?.ontology_term_uri ||
    ''
  );
  const [notes, setNotes] = useState(mapping.notes || '');
  const [showNotes, setShowNotes] = useState(false);

  const primaryCandidate = mapping.candidates[0];
  const hasMultipleCandidates = mapping.candidates.length > 1;

  // Determine current state
  const isApproved = mapping.approval_status === 'approved' || mapping.approval_status === 'auto_approved';
  const isRejected = mapping.approval_status === 'rejected';
  const isModified = mapping.approval_status === 'modified';
  const isPending = mapping.approval_status === 'pending';

  const handleApprove = () => {
    onActionChange(mapping.field_id, 'approve', selectedUri, notes);
  };

  const handleReject = () => {
    onActionChange(mapping.field_id, 'reject', undefined, notes);
  };

  const handleModify = (newUri: string) => {
    setSelectedUri(newUri);
    onActionChange(mapping.field_id, 'modify', newUri, notes);
  };

  const handleNotesChange = (newNotes: string) => {
    setNotes(newNotes);
    // Update notes for current action
    const currentAction: MappingAction = isApproved ? 'approve' : isRejected ? 'reject' : isModified ? 'modify' : 'approve';
    onActionChange(mapping.field_id, currentAction, selectedUri, newNotes);
  };

  // Status badge
  const statusBadge = isApproved ? (
    <Badge className="bg-green-100 text-green-800 border-green-200">
      <Check className="h-3 w-3 mr-1" />
      {mapping.approval_status === 'auto_approved' ? 'Auto-Approved' : 'Approved'}
    </Badge>
  ) : isRejected ? (
    <Badge className="bg-red-100 text-red-800 border-red-200">
      <X className="h-3 w-3 mr-1" />
      Rejected
    </Badge>
  ) : isModified ? (
    <Badge className="bg-blue-100 text-blue-800 border-blue-200">
      <Edit2 className="h-3 w-3 mr-1" />
      Modified
    </Badge>
  ) : (
    <Badge variant="outline" className="bg-yellow-50 text-yellow-800 border-yellow-200">
      Pending Review
    </Badge>
  );

  return (
    <div className="border rounded-lg p-4 space-y-3 hover:border-primary/50 transition-colors">
      {/* Header Row */}
      <div className="flex items-start justify-between gap-4">
        <div className="flex-1 space-y-1">
          <div className="flex items-center gap-2">
            <span className="font-medium text-base">{mapping.field_name}</span>
            <Badge variant="outline" className="text-xs">
              {mapping.data_type}
            </Badge>
            {statusBadge}
          </div>

          {/* Primary suggestion */}
          {primaryCandidate && (
            <div className="flex items-center gap-2 text-sm">
              <span className="text-muted-foreground">Suggested:</span>
              <TooltipProvider>
                <Tooltip>
                  <TooltipTrigger asChild>
                    <code className="bg-muted px-2 py-0.5 rounded text-xs cursor-help">
                      {primaryCandidate.ontology_term_uri.split('/').pop()}
                    </code>
                  </TooltipTrigger>
                  <TooltipContent>
                    <div className="space-y-1 text-xs">
                      <div className="font-semibold">Full URI:</div>
                      <div className="max-w-xs break-all">{primaryCandidate.ontology_term_uri}</div>
                      <div className="pt-1 border-t">
                        <div className="font-semibold">Explanation:</div>
                        <div>{primaryCandidate.explanation}</div>
                      </div>
                    </div>
                  </TooltipContent>
                </Tooltip>
              </TooltipProvider>
              <ConfidenceBadge
                confidence={primaryCandidate.confidence}
                breakdown={primaryCandidate.confidence_breakdown}
                size="sm"
              />
            </div>
          )}
        </div>

        {/* Action Buttons */}
        <div className="flex items-center gap-2">
          {!isRejected && (
            <Button
              size="sm"
              variant={isApproved ? "default" : "outline"}
              onClick={handleApprove}
              disabled={!primaryCandidate}
            >
              <Check className="h-3.5 w-3.5 mr-1" />
              {isApproved ? 'Approved' : 'Approve'}
            </Button>
          )}
          <Button
            size="sm"
            variant={isRejected ? "destructive" : "outline"}
            onClick={handleReject}
          >
            <X className="h-3.5 w-3.5 mr-1" />
            {isRejected ? 'Rejected' : 'Reject'}
          </Button>
          {hasMultipleCandidates && (
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setIsExpanded(!isExpanded)}
            >
              {isExpanded ? (
                <ChevronUp className="h-4 w-4" />
              ) : (
                <ChevronDown className="h-4 w-4" />
              )}
            </Button>
          )}
        </div>
      </div>

      {/* Expanded Details */}
      {isExpanded && (
        <div className="space-y-3 pt-3 border-t">
          {/* Sample Values */}
          {mapping.sample_values.length > 0 && (
            <div>
              <div className="text-xs font-medium text-muted-foreground mb-1 flex items-center gap-1">
                <Info className="h-3 w-3" />
                Sample Values
              </div>
              <div className="flex flex-wrap gap-1.5">
                {mapping.sample_values.slice(0, 5).map((value, idx) => (
                  <Badge key={idx} variant="secondary" className="text-xs font-mono">
                    {value.length > 30 ? value.substring(0, 30) + '...' : value}
                  </Badge>
                ))}
                {mapping.sample_values.length > 5 && (
                  <Badge variant="secondary" className="text-xs">
                    +{mapping.sample_values.length - 5} more
                  </Badge>
                )}
              </div>
            </div>
          )}

          {/* Alternative Suggestions */}
          {hasMultipleCandidates && (
            <div>
              <div className="text-xs font-medium text-muted-foreground mb-2">
                Alternative Suggestions ({mapping.candidates.length})
              </div>
              <Select value={selectedUri} onValueChange={handleModify}>
                <SelectTrigger className="w-full">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  {mapping.candidates.map((candidate) => (
                    <SelectItem key={candidate.ontology_term_uri} value={candidate.ontology_term_uri}>
                      <div className="flex items-center justify-between gap-3 w-full">
                        <span className="text-sm">
                          {candidate.ontology_term_uri.split('/').pop()}
                        </span>
                        <ConfidenceBadge
                          confidence={candidate.confidence}
                          size="sm"
                          showLabel={true}
                          showIcon={false}
                        />
                      </div>
                    </SelectItem>
                  ))}
                </SelectContent>
              </Select>
            </div>
          )}

          {/* Notes Section */}
          <div>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => setShowNotes(!showNotes)}
              className="text-xs"
            >
              {showNotes ? 'Hide' : 'Add'} Notes
            </Button>
            {showNotes && (
              <Textarea
                placeholder="Add notes about this mapping decision..."
                value={notes}
                onChange={(e) => handleNotesChange(e.target.value)}
                className="mt-2 text-sm"
                rows={2}
              />
            )}
          </div>
        </div>
      )}
    </div>
  );
}
