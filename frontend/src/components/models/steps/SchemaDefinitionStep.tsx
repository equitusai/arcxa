import React, { useState } from 'react';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Card, CardContent } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import { Alert, AlertDescription } from '@/components/ui/alert';
import { Switch } from '@/components/ui/switch';
import {
  Sparkles,
  FileJson,
  PenTool,
  Globe,
  Plus,
  Trash2,
  Check,
  AlertCircle,
  Code,
  Loader2,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import type { WizardFormData } from '../RegisterModelWizard';
import type { FeatureSchema, FeatureDataType } from '@/api/types';

interface SchemaDefinitionStepProps {
  formData: WizardFormData;
  updateFormData: (data: Partial<WizardFormData>) => void;
}

type ImportMethod = 'sample' | 'json-schema' | 'endpoint' | 'manual';

export function SchemaDefinitionStep({ formData, updateFormData }: SchemaDefinitionStepProps) {
  const [importMethod, setImportMethod] = useState<ImportMethod>('sample');
  const [sampleJson, setSampleJson] = useState('');
  const [jsonSchema, setJsonSchema] = useState('');
  const [isDetecting, setIsDetecting] = useState(false);
  const [inferenceResult, setInferenceResult] = useState<{
    success: boolean;
    message: string;
    count?: number;
  } | null>(null);
  const [outputFields, setOutputFields] = useState<string>(
    formData.output_schema.join(', ')
  );

  // Infer schema from sample JSON
  const inferSchemaFromSample = () => {
    setInferenceResult(null);
    try {
      const parsed = JSON.parse(sampleJson);
      const schema: FeatureSchema[] = [];

      const inferType = (value: any): FeatureDataType => {
        if (value === null) return 'string';
        if (typeof value === 'string') return 'string';
        if (typeof value === 'number') {
          return Number.isInteger(value) ? 'integer' : 'float';
        }
        if (typeof value === 'boolean') return 'boolean';
        if (Array.isArray(value)) return 'array';
        if (typeof value === 'object') return 'object';
        return 'string';
      };

      const processObject = (obj: any, prefix = '') => {
        Object.entries(obj).forEach(([key, value]) => {
          const fieldName = prefix ? `${prefix}.${key}` : key;
          schema.push({
            name: fieldName,
            data_type: inferType(value),
            required: true,
          });
        });
      };

      processObject(parsed);

      if (schema.length === 0) {
        setInferenceResult({
          success: false,
          message: 'No fields detected in sample JSON',
        });
        return;
      }

      updateFormData({ input_schema: schema });
      setInferenceResult({
        success: true,
        message: `Successfully inferred ${schema.length} input features`,
        count: schema.length,
      });
    } catch (error) {
      setInferenceResult({
        success: false,
        message: 'Invalid JSON format. Please check your input.',
      });
    }
  };

  // Add manual field
  const addManualField = () => {
    updateFormData({
      input_schema: [
        ...formData.input_schema,
        { name: '', data_type: 'string', required: true },
      ],
    });
  };

  // Remove field
  const removeField = (index: number) => {
    const newSchema = formData.input_schema.filter((_, i) => i !== index);
    updateFormData({ input_schema: newSchema });
  };

  // Update field
  const updateField = (
    index: number,
    updates: Partial<FeatureSchema>
  ) => {
    const newSchema = [...formData.input_schema];
    newSchema[index] = { ...newSchema[index], ...updates };
    updateFormData({ input_schema: newSchema });
  };

  // Update output schema
  const handleOutputFieldsChange = (value: string) => {
    setOutputFields(value);
    const fields = value
      .split(',')
      .map(f => f.trim())
      .filter(f => f.length > 0);
    updateFormData({ output_schema: fields });
  };

  // Detect from endpoint
  const detectFromEndpoint = async () => {
    setInferenceResult(null);
    setIsDetecting(true);

    if (!formData.endpoint.url) {
      setInferenceResult({
        success: false,
        message: 'No endpoint URL configured. Please configure the endpoint first.',
      });
      setIsDetecting(false);
      return;
    }

    try {
      // Extract base URL
      const baseUrl = new URL(formData.endpoint.url);
      const origin = baseUrl.origin;
      const basePath = baseUrl.pathname.replace(/\/+$/, ''); // Remove trailing slashes

      // Common schema endpoint paths to probe
      const schemaEndpoints = [
        '/openapi.json',
        '/swagger.json',
        '/docs/openapi.json',
        '/api/openapi.json',
        '/metadata',
        `${basePath}/openapi.json`,
        `${basePath}/swagger.json`,
      ];

      let schemaFound = false;

      // Try each endpoint
      for (const path of schemaEndpoints) {
        try {
          const url = `${origin}${path}`;
          const response = await fetch(url, {
            method: 'GET',
            headers: {
              'Accept': 'application/json',
              ...formData.endpoint.headers,
            },
            signal: AbortSignal.timeout(5000), // 5 second timeout
          });

          if (response.ok) {
            const data = await response.json();

            // Parse OpenAPI spec
            if (data.openapi || data.swagger) {
              const schema = parseOpenAPISchema(data);
              if (schema.input_schema.length > 0) {
                updateFormData({
                  input_schema: schema.input_schema,
                  output_schema: schema.output_schema,
                });
                setInferenceResult({
                  success: true,
                  message: `Schema detected from ${path} (${schema.input_schema.length} inputs, ${schema.output_schema.length} outputs)`,
                  count: schema.input_schema.length,
                });
                schemaFound = true;
                break;
              }
            }
          }
        } catch (err) {
          // Continue to next endpoint
          continue;
        }
      }

      if (!schemaFound) {
        setInferenceResult({
          success: false,
          message: 'No OpenAPI/Swagger schema found at common endpoints. Try manual import instead.',
        });
      }
    } catch (error) {
      setInferenceResult({
        success: false,
        message: error instanceof Error ? error.message : 'Failed to probe endpoint',
      });
    } finally {
      setIsDetecting(false);
    }
  };

  // Parse OpenAPI schema to extract input/output fields
  const parseOpenAPISchema = (spec: any): { input_schema: FeatureSchema[], output_schema: string[] } => {
    const inputSchema: FeatureSchema[] = [];
    const outputSchema: string[] = [];

    try {
      // Find the first POST endpoint (usually the prediction endpoint)
      const paths = spec.paths || {};
      let requestSchema: any = null;
      let responseSchema: any = null;

      for (const path of Object.keys(paths)) {
        const methods = paths[path];
        const postMethod = methods.post || methods.POST;

        if (postMethod) {
          // Extract request schema
          const requestBody = postMethod.requestBody?.content?.['application/json']?.schema;
          if (requestBody) {
            requestSchema = requestBody;
          }

          // Extract response schema
          const successResponse = postMethod.responses?.['200']?.content?.['application/json']?.schema;
          if (successResponse) {
            responseSchema = successResponse;
          }

          if (requestSchema || responseSchema) break;
        }
      }

      // Parse request schema
      if (requestSchema) {
        const properties = requestSchema.properties || {};
        const required = requestSchema.required || [];

        Object.entries(properties).forEach(([name, prop]: [string, any]) => {
          inputSchema.push({
            name,
            data_type: mapOpenAPITypeToFeatureType(prop.type),
            required: required.includes(name),
          });
        });
      }

      // Parse response schema
      if (responseSchema) {
        const properties = responseSchema.properties || {};
        Object.keys(properties).forEach((name) => {
          outputSchema.push(name);
        });
      }
    } catch (error) {
      console.error('Failed to parse OpenAPI schema:', error);
    }

    return { input_schema: inputSchema, output_schema: outputSchema };
  };

  // Map OpenAPI types to feature types
  const mapOpenAPITypeToFeatureType = (type: string): FeatureDataType => {
    switch (type?.toLowerCase()) {
      case 'string':
        return 'string';
      case 'integer':
      case 'int':
      case 'int32':
      case 'int64':
        return 'integer';
      case 'number':
      case 'float':
      case 'double':
        return 'float';
      case 'boolean':
      case 'bool':
        return 'boolean';
      case 'array':
        return 'array';
      case 'object':
        return 'object';
      default:
        return 'string';
    }
  };

  const importMethods = [
    {
      id: 'sample' as const,
      name: 'Import from Sample',
      description: 'Paste sample request JSON',
      icon: FileJson,
      recommended: true,
    },
    {
      id: 'endpoint' as const,
      name: 'Auto-Detect',
      description: 'Probe endpoint for schema',
      icon: Globe,
    },
    {
      id: 'json-schema' as const,
      name: 'JSON Schema',
      description: 'Import OpenAPI/JSON Schema',
      icon: Code,
    },
    {
      id: 'manual' as const,
      name: 'Manual Entry',
      description: 'Build schema from scratch',
      icon: PenTool,
    },
  ];

  return (
    <div className="space-y-6">
      <div>
        <h3 className="text-lg font-semibold text-foreground mb-2">
          Input/Output Schema
        </h3>
        <p className="text-sm text-muted-foreground">
          Define the data structure your model expects
        </p>
      </div>

      {/* Import Method Selector */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
        {importMethods.map((method) => (
          <Card
            key={method.id}
            className={cn(
              'cursor-pointer transition-all hover:shadow-md',
              importMethod === method.id
                ? 'border-entity bg-entity/5'
                : 'hover:border-entity/50'
            )}
            onClick={() => setImportMethod(method.id)}
          >
            <CardContent className="p-3">
              <div className="flex flex-col items-center text-center gap-2">
                <method.icon
                  className={cn(
                    'h-6 w-6',
                    importMethod === method.id
                      ? 'text-entity'
                      : 'text-muted-foreground'
                  )}
                />
                <div>
                  <div className="flex items-center gap-1 justify-center">
                    <span className="text-xs font-semibold">{method.name}</span>
                    {method.recommended && (
                      <Badge variant="outline" className="text-[10px] px-1 py-0">
                        Fast
                      </Badge>
                    )}
                  </div>
                  <p className="text-[10px] text-muted-foreground mt-1">
                    {method.description}
                  </p>
                </div>
              </div>
            </CardContent>
          </Card>
        ))}
      </div>

      {/* Import from Sample JSON */}
      {importMethod === 'sample' && (
        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="sample-json">Sample Request JSON</Label>
            <Textarea
              id="sample-json"
              value={sampleJson}
              onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setSampleJson(e.target.value)}
              placeholder={`{\n  "customer_id": "C12345",\n  "transaction_amount": 150.50,\n  "merchant_category": "retail",\n  "is_international": false\n}`}
              rows={8}
              className="font-mono text-sm"
            />
            <p className="text-xs text-muted-foreground">
              Paste a sample request body. We'll automatically infer field types.
            </p>
          </div>

          <Button onClick={inferSchemaFromSample} className="gap-2" disabled={!sampleJson}>
            <Sparkles className="h-4 w-4" />
            Infer Schema
          </Button>

          {inferenceResult && (
            <Alert variant={inferenceResult.success ? 'default' : 'destructive'}>
              <AlertDescription className="flex items-center gap-2">
                {inferenceResult.success ? (
                  <Check className="h-4 w-4 text-success" />
                ) : (
                  <AlertCircle className="h-4 w-4" />
                )}
                <span>{inferenceResult.message}</span>
              </AlertDescription>
            </Alert>
          )}
        </div>
      )}

      {/* Auto-Detect from Endpoint */}
      {importMethod === 'endpoint' && (
        <div className="space-y-4">
          <Alert>
            <Globe className="h-4 w-4" />
            <AlertDescription>
              {formData.endpoint.url
                ? 'Probes common schema endpoints: /openapi.json, /swagger.json, /metadata'
                : 'Configure an endpoint URL in the previous step to enable auto-detection'}
            </AlertDescription>
          </Alert>

          <Button
            onClick={detectFromEndpoint}
            className="gap-2"
            disabled={!formData.endpoint.url || isDetecting}
          >
            {isDetecting ? (
              <>
                <Loader2 className="h-4 w-4 animate-spin" />
                Detecting Schema...
              </>
            ) : (
              <>
                <Sparkles className="h-4 w-4" />
                Detect Schema
              </>
            )}
          </Button>

          {inferenceResult && (
            <Alert variant={inferenceResult.success ? 'default' : 'destructive'}>
              <AlertDescription className="flex items-center gap-2">
                {inferenceResult.success ? (
                  <Check className="h-4 w-4 text-success" />
                ) : (
                  <AlertCircle className="h-4 w-4" />
                )}
                <span>{inferenceResult.message}</span>
              </AlertDescription>
            </Alert>
          )}
        </div>
      )}

      {/* JSON Schema Import */}
      {importMethod === 'json-schema' && (
        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="json-schema">JSON Schema / OpenAPI Spec</Label>
            <Textarea
              id="json-schema"
              value={jsonSchema}
              onChange={(e: React.ChangeEvent<HTMLTextAreaElement>) => setJsonSchema(e.target.value)}
              placeholder={`{\n  "type": "object",\n  "properties": {\n    "customer_id": { "type": "string" },\n    "amount": { "type": "number" }\n  }\n}`}
              rows={10}
              className="font-mono text-sm"
            />
          </div>

          <Button className="gap-2" disabled>
            <Sparkles className="h-4 w-4" />
            Import Schema (Coming Soon)
          </Button>
        </div>
      )}

      {/* Schema Editor (shows after inference or for manual entry) */}
      {(formData.input_schema.length > 0 || importMethod === 'manual') && (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <Label className="text-base">
              Input Features
              <span className="text-error ml-1">*</span>
              <Badge variant="outline" className="ml-2 font-normal">
                {formData.input_schema.length} fields
              </Badge>
            </Label>
            <Button
              variant="outline"
              size="sm"
              onClick={addManualField}
              className="gap-2"
            >
              <Plus className="h-3 w-3" />
              Add Field
            </Button>
          </div>

          {formData.input_schema.length === 0 ? (
            <Alert>
              <AlertDescription>
                No input features defined. Add fields manually or import from sample JSON.
              </AlertDescription>
            </Alert>
          ) : (
            <div className="space-y-2 max-h-80 overflow-y-auto">
              {formData.input_schema.map((field, index) => (
                <div key={index} className="flex gap-2 items-start p-3 border border-border rounded-md">
                  <div className="flex-1 grid grid-cols-1 md:grid-cols-3 gap-2">
                    <div className="space-y-1">
                      <Label className="text-xs">Field Name</Label>
                      <Input
                        value={field.name}
                        onChange={(e) =>
                          updateField(index, { name: e.target.value })
                        }
                        placeholder="field_name"
                        className="font-mono text-sm"
                      />
                    </div>

                    <div className="space-y-1">
                      <Label className="text-xs">Data Type</Label>
                      <Select
                        value={field.data_type}
                        onValueChange={(value) =>
                          updateField(index, { data_type: value as FeatureDataType })
                        }
                      >
                        <SelectTrigger className="text-sm">
                          <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="string">String</SelectItem>
                          <SelectItem value="integer">Integer</SelectItem>
                          <SelectItem value="float">Float</SelectItem>
                          <SelectItem value="boolean">Boolean</SelectItem>
                          <SelectItem value="array">Array</SelectItem>
                          <SelectItem value="object">Object</SelectItem>
                        </SelectContent>
                      </Select>
                    </div>

                    <div className="space-y-1">
                      <Label className="text-xs">Required</Label>
                      <div className="flex items-center h-9 px-3 border border-border rounded-md">
                        <Switch
                          checked={field.required}
                          onCheckedChange={(checked) =>
                            updateField(index, { required: checked })
                          }
                        />
                        <span className="ml-2 text-xs text-muted-foreground">
                          {field.required ? 'Yes' : 'Optional'}
                        </span>
                      </div>
                    </div>
                  </div>

                  <Button
                    variant="ghost"
                    size="sm"
                    onClick={() => removeField(index)}
                    className="px-2 mt-6"
                  >
                    <Trash2 className="h-4 w-4 text-error" />
                  </Button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      {/* Output Schema */}
      <div className="space-y-2">
        <Label htmlFor="output-fields">
          Output Fields <span className="text-error">*</span>
        </Label>
        <Input
          id="output-fields"
          value={outputFields}
          onChange={(e) => handleOutputFieldsChange(e.target.value)}
          placeholder="prediction, confidence_score, risk_level"
          className="font-mono text-sm"
        />
        <p className="text-xs text-muted-foreground">
          Comma-separated list of output field names (e.g., prediction, score)
        </p>
        {formData.output_schema.length > 0 && (
          <div className="flex items-center gap-2 flex-wrap">
            {formData.output_schema.map((field, i) => (
              <Badge key={i} variant="outline">
                {field}
              </Badge>
            ))}
          </div>
        )}
      </div>

      {/* Schema Summary */}
      {formData.input_schema.length > 0 && formData.output_schema.length > 0 && (
        <Alert className="bg-success/10 border-success/20">
          <Check className="h-4 w-4 text-success" />
          <AlertDescription>
            <span className="font-semibold text-foreground">Schema validated:</span>{' '}
            {formData.input_schema.length} input features →{' '}
            {formData.output_schema.length} output fields
          </AlertDescription>
        </Alert>
      )}
    </div>
  );
}
