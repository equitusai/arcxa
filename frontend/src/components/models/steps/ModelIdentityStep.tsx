import React, { useState } from 'react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Button } from '@/components/ui/button';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Check, FileText, Upload, Sparkles } from 'lucide-react';
import { cn } from '@/lib/utils';
import { MODEL_TEMPLATES, type ModelTemplate } from '@/lib/model-templates';
import type { WizardFormData } from '../RegisterModelWizard';

interface ModelIdentityStepProps {
  formData: WizardFormData;
  updateFormData: (data: Partial<WizardFormData>) => void;
}

export function ModelIdentityStep({ formData, updateFormData }: ModelIdentityStepProps) {
  const [showTemplates, setShowTemplates] = useState(true);
  const [selectedTemplate, setSelectedTemplate] = useState<string | null>(null);

  const handleTemplateSelect = (template: ModelTemplate) => {
    setSelectedTemplate(template.id);
    updateFormData({
      ...template.defaults,
      framework: template.defaults.framework || 'custom',
    });
    setShowTemplates(false);
  };

  const handleManualEntry = () => {
    setShowTemplates(false);
    setSelectedTemplate(null);
  };

  const generateModelId = () => {
    const prefix = 'mdl';
    const random = Math.random().toString(36).substring(2, 9);
    updateFormData({ id: `${prefix}_${random}` });
  };

  React.useEffect(() => {
    if (!formData.id) {
      generateModelId();
    }
  }, []);

  if (showTemplates) {
    return (
      <div className="space-y-6">
        <div>
          <h3 className="text-lg font-semibold text-foreground mb-2">Quick Start</h3>
          <p className="text-sm text-muted-foreground">
            Choose a template to get started quickly, or start from scratch
          </p>
        </div>

        {/* Template Grid */}
        <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
          {MODEL_TEMPLATES.map((template) => (
            <Card
              key={template.id}
              className="cursor-pointer hover:border-entity transition-all hover:shadow-md"
              onClick={() => handleTemplateSelect(template)}
            >
              <CardContent className="p-4">
                <div className="flex items-start gap-3">
                  <div className="text-3xl">{template.icon}</div>
                  <div className="flex-1 min-w-0">
                    <h4 className="font-semibold text-foreground mb-1">{template.name}</h4>
                    <p className="text-sm text-muted-foreground mb-2">{template.description}</p>
                    <div className="flex items-center gap-2">
                      <Badge variant="outline" className="text-xs">
                        {template.category}
                      </Badge>
                      <Badge variant="outline" className="text-xs">
                        {template.defaults.framework}
                      </Badge>
                    </div>
                  </div>
                </div>
              </CardContent>
            </Card>
          ))}
        </div>

        {/* Manual Entry Option */}
        <div className="flex items-center gap-4 pt-4">
          <div className="flex-1 border-t border-border" />
          <span className="text-xs text-muted-foreground font-semibold">OR</span>
          <div className="flex-1 border-t border-border" />
        </div>

        <div className="flex justify-center gap-3">
          <Button variant="outline" className="gap-2" disabled>
            <Upload className="h-4 w-4" />
            Import from File
          </Button>
          <Button variant="outline" onClick={handleManualEntry} className="gap-2">
            <FileText className="h-4 w-4" />
            Manual Entry
          </Button>
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      {selectedTemplate && (
        <div className="flex items-center gap-2 p-3 bg-success/10 border border-success/20 rounded-md">
          <Check className="h-4 w-4 text-success" />
          <span className="text-sm font-semibold text-foreground">
            Using template: {MODEL_TEMPLATES.find(t => t.id === selectedTemplate)?.name}
          </span>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => setShowTemplates(true)}
            className="ml-auto text-xs"
          >
            Change Template
          </Button>
        </div>
      )}

      <div className="space-y-4">
        {/* Model Name */}
        <div className="space-y-2">
          <Label htmlFor="name" className="flex items-center gap-2">
            Model Name <span className="text-error">*</span>
          </Label>
          <Input
            id="name"
            value={formData.name}
            onChange={(e) => updateFormData({ name: e.target.value })}
            placeholder="e.g., fraud-detection-xgboost-v3"
            className="font-mono"
          />
          <p className="text-xs text-muted-foreground">
            Use descriptive names: {'{team}'}-{'{purpose}'}-{'{framework}'}
          </p>
        </div>

        {/* Model ID */}
        <div className="space-y-2">
          <Label htmlFor="id" className="flex items-center gap-2">
            Model ID
            <Badge variant="outline" className="text-xs font-normal">Auto-generated</Badge>
          </Label>
          <div className="flex gap-2">
            <Input
              id="id"
              value={formData.id}
              onChange={(e) => updateFormData({ id: e.target.value })}
              className="font-mono text-sm"
              readOnly
            />
            <Button
              variant="outline"
              size="sm"
              onClick={generateModelId}
              title="Regenerate ID"
            >
              <Sparkles className="h-4 w-4" />
            </Button>
          </div>
        </div>

        {/* Version */}
        <div className="space-y-2">
          <Label htmlFor="version">Version</Label>
          <Input
            id="version"
            value={formData.version}
            onChange={(e) => updateFormData({ version: e.target.value })}
            placeholder="1.0.0"
          />
        </div>

        {/* Framework */}
        <div className="space-y-2">
          <Label htmlFor="framework">
            Serving Framework <span className="text-error">*</span>
          </Label>
          <Select
            value={formData.framework}
            onValueChange={(value) => updateFormData({ framework: value })}
          >
            <SelectTrigger id="framework">
              <SelectValue />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="tensorflow">TensorFlow Serving</SelectItem>
              <SelectItem value="torch">TorchServe</SelectItem>
              <SelectItem value="sagemaker">AWS SageMaker</SelectItem>
              <SelectItem value="custom">Custom</SelectItem>
            </SelectContent>
          </Select>
        </div>

        {/* Description - Optional */}
        <div className="space-y-2">
          <Label htmlFor="description">
            Description
            <span className="text-xs text-muted-foreground ml-2">(optional)</span>
          </Label>
          <Textarea
            id="description"
            value={formData.description}
            onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => updateFormData({ description: e.target.value })}
            placeholder="Brief description of what this model does..."
            rows={3}
          />
        </div>
      </div>
    </div>
  );
}
