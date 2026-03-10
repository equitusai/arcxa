import * as React from "react"
import { Slot } from "@radix-ui/react-slot"
import { cva, type VariantProps } from "class-variance-authority"

import { cn } from "@/lib/utils"

const buttonVariants = cva(
  "inline-flex items-center justify-center whitespace-nowrap rounded-sm text-sm font-semibold transition-colors duration-150 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-0 disabled:pointer-events-none disabled:opacity-40",
  {
    variants: {
      variant: {
        default: "bg-primary text-primary-foreground hover:bg-primary-hover border border-primary shadow-none",
        destructive:
          "bg-destructive text-destructive-foreground hover:bg-destructive/90 border border-destructive shadow-none",
        outline:
          "border border-border bg-background hover:bg-background-secondary text-foreground shadow-none",
        secondary:
          "bg-accent text-accent-foreground hover:bg-accent/90 border border-accent shadow-none",
        ghost: "hover:bg-background-secondary text-foreground-secondary hover:text-foreground",
        link: "text-accent underline-offset-4 hover:underline hover:text-accent/80",
      },
      size: {
        default: "h-8 px-4 py-1.5 text-sm",
        sm: "h-7 px-3 text-xs",
        lg: "h-9 px-5 text-sm",
        icon: "h-8 w-8",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  }
)

export interface ButtonProps
  extends React.ButtonHTMLAttributes<HTMLButtonElement>,
    VariantProps<typeof buttonVariants> {
  asChild?: boolean
}

const Button = React.forwardRef<HTMLButtonElement, ButtonProps>(
  ({ className, variant, size, asChild = false, ...props }, ref) => {
    const Comp = asChild ? Slot : "button"
    return (
      <Comp
        className={cn(buttonVariants({ variant, size, className }))}
        ref={ref}
        {...props}
      />
    )
  }
)
Button.displayName = "Button"

export { Button, buttonVariants }