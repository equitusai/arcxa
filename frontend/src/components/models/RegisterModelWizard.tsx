import React, { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Progress } from '@/components/ui/progress';
import { ChevronLeft, ChevronRight, Check } from 'lucide-react';
import { cn } from '@/lib/utils';
import { ModelIdentityStep } from './steps/ModelIdentityStep';
import { EndpointConfigStep } from './steps/EndpointConfigStep';
import { SchemaDefinitionStep } from './steps/SchemaDefinitionStep';
import { ResilienceConfigStep } from './steps/ResilienceConfigStep';
import { TestDeployStep } from './steps/TestDeployStep';
import type { RegisterModelRequest, FeatureSchema } from '@/api/types';

interface RegisterModelWizardProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export interface WizardFormData {
  // Step 1: Identity
  id: string;
  name: string;
  version: string;
  description: string;
  framework: string;
  tags: string[];

  // Step 2: Endpoint
  endpoint: {
    protocol: string;
    url: string;
    timeout_ms: number;
    headers: Record<string, string>;
  };

  // Step 3: Schema
  input_schema: FeatureSchema[];
  output_schema: string[];

  // Step 4: Resilience
  circuitBreaker: {
    enabled: boolean;
    failureThreshold: number;
    successThreshold: number;
    timeoutMs: number;
  };
  retry: {
    enabled: boolean;
    maxAttempts: number;
  };
  cache: {
    enabled: boolean;
    ttlSeconds: number;
  };
}

const STEPS = [
  { id: 1, name: 'Identity', description: 'Model information' },
  { id: 2, name: 'Endpoint', description: 'Connection settings' },
  { id: 3, name: 'Schema', description: 'Input/Output schema' },
  { id: 4, name: 'Resilience', description: 'Circuit breaker & retry' },
  { id: 5, name: 'Deploy', description: 'Test & deploy' },
];

export function RegisterModelWizard({ open, onOpenChange }: RegisterModelWizardProps) {
  const [currentStep, setCurrentStep] = useState(1);
  const [formData, setFormData] = useState<WizardFormData>({
    id: '',
    name: '',
    version: '1.0.0',
    description: '',
    framework: 'tensorflow',
    tags: [],
    endpoint: {
      protocol: 'http',
      url: '',
      timeout_ms: 5000,
      headers: {},
    },
    input_schema: [],
    output_schema: [],
    circuitBreaker: {
      enabled: true,
      failureThreshold: 5,
      successThreshold: 2,
      timeoutMs: 30000,
    },
    retry: {
      enabled: true,
      maxAttempts: 3,
    },
    cache: {
      enabled: true,
      ttlSeconds: 300,
    },
  });

  const updateFormData = (updates: Partial<WizardFormData>) => {
    setFormData(prev => ({ ...prev, ...updates }));
  };

  const handleNext = () => {
    if (currentStep < STEPS.length) {
      setCurrentStep(prev => prev + 1);
    }
  };

  const handlePrevious = () => {
    if (currentStep > 1) {
      setCurrentStep(prev => prev - 1);
    }
  };

  const handleClose = () => {
    setCurrentStep(1);
    onOpenChange(false);
  };

  const progress = (currentStep / STEPS.length) * 100;

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-4xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="text-xl">Register ML Model</DialogTitle>
          <DialogDescription>
            Set up a new production-ready model with enterprise resilience
          </DialogDescription>
        </DialogHeader>

        {/* Progress Bar */}
        <div className="space-y-2">
          <Progress value={progress} className="h-2" />

          {/* Step Indicators */}
          <div className="flex justify-between">
            {STEPS.map((step) => (
              <div
                key={step.id}
                className={cn(
                  "flex flex-col items-center flex-1 relative",
                  step.id < STEPS.length && "after:absolute after:top-3 after:left-[60%] after:w-full after:h-0.5 after:bg-border"
                )}
              >
                <div
                  className={cn(
                    "w-6 h-6 rounded-full border-2 flex items-center justify-center text-xs font-semibold transition-colors z-10 bg-background",
                    currentStep === step.id && "border-entity bg-entity text-white",
                    currentStep > step.id && "border-success bg-success text-white",
                    currentStep < step.id && "border-border text-muted-foreground"
                  )}
                >
                  {currentStep > step.id ? (
                    <Check className="h-3 w-3" />
                  ) : (
                    step.id
                  )}
                </div>
                <div className="text-center mt-1">
                  <p className={cn(
                    "text-xs font-semibold",
                    currentStep >= step.id ? "text-foreground" : "text-muted-foreground"
                  )}>
                    {step.name}
                  </p>
                  <p className="text-[10px] text-muted-foreground hidden sm:block">
                    {step.description}
                  </p>
                </div>
              </div>
            ))}
          </div>
        </div>

        {/* Step Content */}
        <div className="flex-1 overflow-y-auto py-4">
          {currentStep === 1 && (
            <ModelIdentityStep formData={formData} updateFormData={updateFormData} />
          )}
          {currentStep === 2 && (
            <EndpointConfigStep formData={formData} updateFormData={updateFormData} />
          )}
          {currentStep === 3 && (
            <SchemaDefinitionStep formData={formData} updateFormData={updateFormData} />
          )}
          {currentStep === 4 && (
            <ResilienceConfigStep formData={formData} updateFormData={updateFormData} />
          )}
          {currentStep === 5 && (
            <TestDeployStep formData={formData} onClose={handleClose} />
          )}
        </div>

        {/* Navigation Buttons */}
        {currentStep < 5 && (
          <div className="flex justify-between pt-4 border-t border-border">
            <Button
              variant="outline"
              onClick={handlePrevious}
              disabled={currentStep === 1}
            >
              <ChevronLeft className="h-4 w-4 mr-1" />
              Previous
            </Button>
            <Button onClick={handleNext}>
              Next
              <ChevronRight className="h-4 w-4 ml-1" />
            </Button>
          </div>
        )}
      </DialogContent>
    </Dialog>
  );
}
