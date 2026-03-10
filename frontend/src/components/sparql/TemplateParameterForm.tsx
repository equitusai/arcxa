/**
 * Template Parameter Form
 *
 * Dynamic form for filling template parameters with smart input types
 */

import React, { useState } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Slider } from '@/components/ui/slider';
import { Badge } from '@/components/ui/badge';
import { Wand2, Code2 } from 'lucide-react';
import type { SparqlTemplate, SparqlTemplateParameter } from '@/api/types';
import { substituteParameters } from '@/api/sparql';
import { cn } from '@/lib/utils';

interface TemplateParameterFormProps {
  template: SparqlTemplate;
  onGenerate: (query: string, params: Record<string, any>) => void;
  className?: string;
}

export function TemplateParameterForm({
  template,
  onGenerate,
  className,
}: TemplateParameterFormProps) {
  const [params, setParams] = useState<Record<string, any>>(() => {
    const initial: Record<string, any> = {};
    template.parameters.forEach(p => {
      if (p.defaultValue !== undefined) {
        initial[p.name] = p.defaultValue;
      }
    });
    return initial;
  });

  const [errors, setErrors] = useState<Record<string, string>>({});

  const handleParamChange = (name: string, value: any) => {
    setParams(prev => ({ ...prev, [name]: value }));
    // Clear error when user starts typing
    if (errors[name]) {
      setErrors(prev => {
        const next = { ...prev };
        delete next[name];
        return next;
      });
    }
  };

  const validateAndGenerate = () => {
    const newErrors: Record<string, string> = {};

    // Check required fields
    template.parameters.forEach(p => {
      if (p.required && !params[p.name]) {
        newErrors[p.name] = `${p.label} is required`;
      }
    });

    if (Object.keys(newErrors).length > 0) {
      setErrors(newErrors);
      return;
    }

    const generatedQuery = substituteParameters(template.sparql, params);
    onGenerate(generatedQuery, params);
  };

  return (
    <Card className={cn('glass-morphism border-border', className)}>
      <CardHeader className="pb-4">
        <CardTitle className="text-base">{template.name}</CardTitle>
        <CardDescription className="text-sm leading-relaxed">
          {template.description}
        </CardDescription>
        {template.exampleResults && (
          <div className="mt-2">
            <Badge variant="outline" className="text-xs">
              {template.exampleResults}
            </Badge>
          </div>
        )}
      </CardHeader>
      <CardContent className="space-y-4">
        {template.parameters.length === 0 ? (
          <p className="text-sm text-muted-foreground">
            No parameters required. Click Generate to create the query.
          </p>
        ) : (
          <>
            <div className="text-xs font-semibold text-muted-foreground uppercase tracking-wide">
              Parameters
            </div>
            {template.parameters.map(param => (
              <div key={param.name} className="space-y-2">
                <Label htmlFor={param.name}>
                  {param.label}
                  {param.required && <span className="text-error ml-1">*</span>}
                </Label>
                <ParameterInput
                  parameter={param}
                  value={params[param.name]}
                  onChange={(val) => handleParamChange(param.name, val)}
                  error={errors[param.name]}
                />
                {param.helpText && (
                  <p className="text-xs text-muted-foreground">
                    ℹ️ {param.helpText}
                  </p>
                )}
                {errors[param.name] && (
                  <p className="text-xs text-error">
                    {errors[param.name]}
                  </p>
                )}
              </div>
            ))}
          </>
        )}

        <div className="pt-2 flex gap-2">
          <Button
            className="flex-1 gap-2"
            onClick={validateAndGenerate}
          >
            <Wand2 className="h-4 w-4" />
            Generate Query
          </Button>
          <Button
            variant="outline"
            className="gap-2"
            onClick={() => {
              const query = substituteParameters(template.sparql, params);
              navigator.clipboard.writeText(query);
            }}
          >
            <Code2 className="h-4 w-4" />
            Copy
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

// ============================================================================
// Parameter Input Components
// ============================================================================

interface ParameterInputProps {
  parameter: SparqlTemplateParameter;
  value: any;
  onChange: (value: any) => void;
  error?: string;
}

function ParameterInput({ parameter, value, onChange, error }: ParameterInputProps) {
  switch (parameter.type) {
    case 'threshold':
      return (
        <div className="space-y-3">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium">{value?.toFixed(2) || '0.70'}</span>
            <span className="text-xs text-muted-foreground">0.0 - 1.0</span>
          </div>
          <Slider
            min={0}
            max={1}
            step={0.01}
            value={[value || 0.7]}
            onValueChange={([val]) => onChange(val)}
            className="w-full"
          />
          <div className="flex justify-between text-xs text-muted-foreground">
            <span>Low</span>
            <span>High</span>
          </div>
        </div>
      );

    case 'number':
      return (
        <Input
          type="number"
          value={value || ''}
          onChange={(e) => onChange(parseInt(e.target.value) || 0)}
          placeholder={parameter.placeholder}
          className={cn(error && 'border-error')}
        />
      );

    case 'date':
      return (
        <Input
          type="date"
          value={value || ''}
          onChange={(e) => onChange(e.target.value)}
          className={cn(error && 'border-error')}
        />
      );

    case 'entity_id':
    case 'model_id':
    case 'text':
    default:
      return (
        <Input
          type="text"
          value={value || ''}
          onChange={(e) => onChange(e.target.value)}
          placeholder={parameter.placeholder}
          className={cn(error && 'border-error')}
        />
      );
  }
}
