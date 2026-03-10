import * as React from "react"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const badgeVariants = cva(
  "inline-flex items-center rounded-sm border px-2 py-0.5 text-xs font-semibold transition-colors focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-0",
  {
    variants: {
      variant: {
        default:
          "border-primary/30 bg-primary/10 text-primary hover:bg-primary/20",
        secondary:
          "border-accent/30 bg-accent/10 text-accent hover:bg-accent/20",
        destructive:
          "border-destructive/30 bg-destructive/10 text-destructive hover:bg-destructive/20",
        outline: "text-foreground border-border bg-background",
        success:
          "border-success/30 bg-success/10 text-success hover:bg-success/20",
        warning:
          "border-warning/30 bg-warning/10 text-warning hover:bg-warning/20",
        entity:
          "border-entity/30 bg-entity/10 text-entity hover:bg-entity/20",
        model:
          "border-model/30 bg-model/10 text-model hover:bg-model/20",
      },
    },
    defaultVariants: {
      variant: "default",
    },
  }
)

export interface BadgeProps
  extends React.HTMLAttributes<HTMLDivElement>,
    VariantProps<typeof badgeVariants> {}

function Badge({ className, variant, ...props }: BadgeProps) {
  return (
    <div className={cn(badgeVariants({ variant }), className)} {...props} />
  )
}

export { Badge, badgeVariants }
