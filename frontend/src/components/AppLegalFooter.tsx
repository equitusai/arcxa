import { APP_COPYRIGHT } from '@/lib/branding';
import { cn } from '@/lib/utils';

interface AppLegalFooterProps {
  centered?: boolean;
  className?: string;
}

export function AppLegalFooter({
  centered = false,
  className,
}: AppLegalFooterProps) {
  return (
    <div
      className={cn(
        'text-[11px] tracking-[0.08em] text-foreground-muted/70',
        centered && 'text-center',
        className
      )}
    >
      {APP_COPYRIGHT}
    </div>
  );
}
