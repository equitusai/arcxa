/**
 * Inline Badge Toggle Component
 * Click to cycle through states (e.g., Strict → Warn → Skip)
 */

import React from 'react';
import { ChevronDown } from 'lucide-react';
import { cn } from '@/lib/utils';

export interface BadgeOption {
  value: string;
  label: string;
  color: 'success' | 'warning' | 'danger' | 'secondary';
}

interface InlineBadgeToggleProps {
  value: string;
  options: BadgeOption[];
  onChange: (value: string) => void;
  disabled?: boolean;
  className?: string;
}

const colorStyles = {
  success: {
    bg: 'bg-green-50',
    border: 'border-green-500',
    text: 'text-green-700',
    hover: 'hover:bg-green-100',
  },
  warning: {
    bg: 'bg-amber-50',
    border: 'border-amber-500 border-dashed',
    text: 'text-amber-700',
    hover: 'hover:bg-amber-100',
  },
  danger: {
    bg: 'bg-red-50',
    border: 'border-red-500',
    text: 'text-red-700',
    hover: 'hover:bg-red-100',
  },
  secondary: {
    bg: 'bg-neutral-50',
    border: 'border-neutral-400 border-dotted',
    text: 'text-neutral-700',
    hover: 'hover:bg-neutral-100',
  },
};

export function InlineBadgeToggle({
  value,
  options,
  onChange,
  disabled = false,
  className,
}: InlineBadgeToggleProps) {
  const currentOption = options.find(opt => opt.value === value) || options[0];
  const currentStyle = colorStyles[currentOption.color];

  const handleClick = () => {
    if (disabled) return;

    const currentIndex = options.findIndex(opt => opt.value === value);
    const nextIndex = (currentIndex + 1) % options.length;
    onChange(options[nextIndex].value);
  };

  return (
    <button
      onClick={handleClick}
      disabled={disabled}
      className={cn(
        'inline-flex items-center gap-1.5 px-2 py-1 rounded border text-xs font-medium transition-colors',
        currentStyle.bg,
        currentStyle.border,
        currentStyle.text,
        !disabled && currentStyle.hover,
        disabled && 'opacity-50 cursor-not-allowed',
        className
      )}
      title={`Click to change (${options.map(o => o.label).join(' → ')})`}
    >
      <span>{currentOption.label}</span>
      <ChevronDown className="w-3 h-3" />
    </button>
  );
}
