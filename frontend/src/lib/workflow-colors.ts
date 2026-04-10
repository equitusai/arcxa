export type WorkflowColorCategory =
  | 'extract'
  | 'transform'
  | 'quality'
  | 'load'
  | 'orchestration'
  | 'prediction'
  | 'logic'
  | 'aggregation'
  | 'routing'
  | 'transformation';

export interface WorkflowStepColor {
  base: string;
  subtle: string;
  surface: string;
  border: string;
  text: string;
  contrast: string;
}

function toWorkflowColor(prefix: string): WorkflowStepColor {
  return {
    base: `hsl(var(--workflow-${prefix}-base))`,
    subtle: `hsl(var(--workflow-${prefix}-subtle))`,
    surface: `hsl(var(--workflow-${prefix}-surface))`,
    border: `hsl(var(--workflow-${prefix}-border))`,
    text: `hsl(var(--workflow-${prefix}-text))`,
    contrast: `hsl(var(--workflow-${prefix}-contrast))`,
  };
}

export function getWorkflowCategoryColor(category: WorkflowColorCategory): WorkflowStepColor {
  return toWorkflowColor(category);
}

