/**
 * FieldMappingTable Component
 *
 * Table view showing all field mappings for review with filtering and bulk actions.
 */

import { useState, useMemo } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Badge } from '@/components/ui/badge';
import { ScrollArea } from '@/components/ui/scroll-area';
import { MappingRow } from './MappingRow';
import { Search, Check, X, Filter } from 'lucide-react';
import type { FieldMapping, MappingAction, FieldApprovalStatus } from '@/api/field-mapping';

interface FieldMappingTableProps {
  mappings: FieldMapping[];
  onActionChange: (fieldId: string, action: MappingAction, selectedUri?: string, notes?: string) => void;
  onBulkApprove?: () => void;
  onBulkReject?: () => void;
}

export function FieldMappingTable({
  mappings,
  onActionChange,
  onBulkApprove,
  onBulkReject,
}: FieldMappingTableProps) {
  const [searchQuery, setSearchQuery] = useState('');
  const [statusFilter, setStatusFilter] = useState<FieldApprovalStatus | 'all'>('all');
  const [confidenceFilter, setConfidenceFilter] = useState<'all' | 'high' | 'medium' | 'low'>('all');

  // Filter and search mappings
  const filteredMappings = useMemo(() => {
    return mappings.filter((mapping) => {
      // Search filter
      const matchesSearch =
        !searchQuery ||
        mapping.field_name.toLowerCase().includes(searchQuery.toLowerCase()) ||
        mapping.data_type.toLowerCase().includes(searchQuery.toLowerCase()) ||
        mapping.candidates.some(c =>
          c.ontology_term_uri.toLowerCase().includes(searchQuery.toLowerCase())
        );

      // Status filter
      const matchesStatus =
        statusFilter === 'all' ||
        mapping.approval_status === statusFilter;

      // Confidence filter
      const confidence = mapping.candidates[0]?.confidence || 0;
      const matchesConfidence =
        confidenceFilter === 'all' ||
        (confidenceFilter === 'high' && confidence >= 0.9) ||
        (confidenceFilter === 'medium' && confidence >= 0.7 && confidence < 0.9) ||
        (confidenceFilter === 'low' && confidence < 0.7);

      return matchesSearch && matchesStatus && matchesConfidence;
    });
  }, [mappings, searchQuery, statusFilter, confidenceFilter]);

  // Calculate stats
  const stats = useMemo(() => {
    const total = mappings.length;
    const autoApproved = mappings.filter(m => m.approval_status === 'auto_approved').length;
    const userApproved = mappings.filter(m => m.approval_status === 'approved').length;
    const pending = mappings.filter(m => m.approval_status === 'pending').length;
    const rejected = mappings.filter(m => m.approval_status === 'rejected').length;
    const modified = mappings.filter(m => m.approval_status === 'modified').length;

    return {
      total,
      autoApproved,
      userApproved,
      pending,
      rejected,
      modified,
      completed: autoApproved + userApproved + modified,
      completionPercentage: total > 0 ? Math.round(((autoApproved + userApproved + modified) / total) * 100) : 0,
    };
  }, [mappings]);

  // Group by status for easier review
  const pendingMappings = filteredMappings.filter(m => m.approval_status === 'pending');
  const approvedMappings = filteredMappings.filter(m =>
    m.approval_status === 'approved' || m.approval_status === 'auto_approved' || m.approval_status === 'modified'
  );
  const rejectedMappings = filteredMappings.filter(m => m.approval_status === 'rejected');

  return (
    <div className="space-y-4">
      {/* Summary Stats */}
      <div className="bg-muted/50 rounded-lg p-4 space-y-3">
        <div className="flex items-center justify-between">
          <h3 className="font-semibold">Mapping Progress</h3>
          <Badge variant="outline" className="text-sm">
            {stats.completed} / {stats.total} Complete ({stats.completionPercentage}%)
          </Badge>
        </div>

        {/* Progress Bar */}
        <div className="w-full bg-muted rounded-full h-2">
          <div
            className="bg-primary rounded-full h-2 transition-all"
            style={{ width: `${stats.completionPercentage}%` }}
          />
        </div>

        {/* Stat Badges */}
        <div className="flex flex-wrap gap-2 text-sm">
          <Badge variant="secondary" className="bg-green-100 text-green-800">
            {stats.autoApproved} Auto-Approved
          </Badge>
          <Badge variant="secondary" className="bg-blue-100 text-blue-800">
            {stats.userApproved} User Approved
          </Badge>
          <Badge variant="secondary" className="bg-yellow-100 text-yellow-800">
            {stats.pending} Pending
          </Badge>
          {stats.rejected > 0 && (
            <Badge variant="secondary" className="bg-red-100 text-red-800">
              {stats.rejected} Rejected
            </Badge>
          )}
          {stats.modified > 0 && (
            <Badge variant="secondary" className="bg-purple-100 text-purple-800">
              {stats.modified} Modified
            </Badge>
          )}
        </div>
      </div>

      {/* Filters and Actions */}
      <div className="flex flex-col sm:flex-row gap-3">
        {/* Search */}
        <div className="flex-1 relative">
          <Search className="absolute left-3 top-1/2 transform -translate-y-1/2 h-4 w-4 text-muted-foreground" />
          <Input
            placeholder="Search fields..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="pl-9"
          />
        </div>

        {/* Filters */}
        <div className="flex gap-2 items-center">
          <Filter className="h-4 w-4 text-muted-foreground hidden sm:block" />

          <Select value={statusFilter} onValueChange={(value) => setStatusFilter(value as typeof statusFilter)}>
            <SelectTrigger className="w-[140px]">
              <SelectValue placeholder="Status" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Status</SelectItem>
              <SelectItem value="pending">Pending</SelectItem>
              <SelectItem value="approved">Approved</SelectItem>
              <SelectItem value="auto_approved">Auto-Approved</SelectItem>
              <SelectItem value="rejected">Rejected</SelectItem>
              <SelectItem value="modified">Modified</SelectItem>
            </SelectContent>
          </Select>

          <Select value={confidenceFilter} onValueChange={(value) => setConfidenceFilter(value as typeof confidenceFilter)}>
            <SelectTrigger className="w-[140px]">
              <SelectValue placeholder="Confidence" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">All Confidence</SelectItem>
              <SelectItem value="high">High (≥90%)</SelectItem>
              <SelectItem value="medium">Medium (70-89%)</SelectItem>
              <SelectItem value="low">Low (&lt;70%)</SelectItem>
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Bulk Actions */}
      {stats.pending > 0 && (
        <div className="flex gap-2">
          <Button
            size="sm"
            variant="outline"
            onClick={onBulkApprove}
            disabled={!onBulkApprove}
          >
            <Check className="h-3.5 w-3.5 mr-2" />
            Approve All Pending ({stats.pending})
          </Button>
          <Button
            size="sm"
            variant="outline"
            onClick={onBulkReject}
            disabled={!onBulkReject}
          >
            <X className="h-3.5 w-3.5 mr-2" />
            Reject All Pending
          </Button>
        </div>
      )}

      {/* Mappings List */}
      <ScrollArea className="h-[500px] pr-4">
        <div className="space-y-4">
          {/* Pending Section */}
          {pendingMappings.length > 0 && (
            <div>
              <div className="text-sm font-semibold text-muted-foreground mb-2 flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-yellow-500" />
                Pending Review ({pendingMappings.length})
              </div>
              <div className="space-y-3">
                {pendingMappings.map((mapping) => (
                  <MappingRow
                    key={mapping.field_id}
                    mapping={mapping}
                    onActionChange={onActionChange}
                    defaultExpanded={false}
                  />
                ))}
              </div>
            </div>
          )}

          {/* Approved Section */}
          {approvedMappings.length > 0 && (
            <div>
              <div className="text-sm font-semibold text-muted-foreground mb-2 flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-green-500" />
                Approved ({approvedMappings.length})
              </div>
              <div className="space-y-3">
                {approvedMappings.map((mapping) => (
                  <MappingRow
                    key={mapping.field_id}
                    mapping={mapping}
                    onActionChange={onActionChange}
                    defaultExpanded={false}
                  />
                ))}
              </div>
            </div>
          )}

          {/* Rejected Section */}
          {rejectedMappings.length > 0 && (
            <div>
              <div className="text-sm font-semibold text-muted-foreground mb-2 flex items-center gap-2">
                <span className="w-2 h-2 rounded-full bg-red-500" />
                Rejected ({rejectedMappings.length})
              </div>
              <div className="space-y-3">
                {rejectedMappings.map((mapping) => (
                  <MappingRow
                    key={mapping.field_id}
                    mapping={mapping}
                    onActionChange={onActionChange}
                    defaultExpanded={false}
                  />
                ))}
              </div>
            </div>
          )}

          {/* Empty State */}
          {filteredMappings.length === 0 && (
            <div className="text-center py-12 text-muted-foreground">
              <p>No mappings match your filters.</p>
              <Button
                variant="link"
                onClick={() => {
                  setSearchQuery('');
                  setStatusFilter('all');
                  setConfidenceFilter('all');
                }}
                className="mt-2"
              >
                Clear Filters
              </Button>
            </div>
          )}
        </div>
      </ScrollArea>
    </div>
  );
}
