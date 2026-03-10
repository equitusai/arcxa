import { APP_NAME } from '@/lib/branding';
import { cn } from '@/lib/utils';

interface BrandMarkProps {
  compact?: boolean;
  centered?: boolean;
  subtitle?: string;
  className?: string;
}

export function BrandMark({
  compact = false,
  centered = false,
  subtitle,
  className,
}: BrandMarkProps) {
  return (
    <div
      className={cn(
        'flex items-center gap-3',
        centered && 'justify-center text-center',
        className
      )}
    >
      <div className="relative flex h-10 w-10 items-center justify-center rounded-md border border-primary/80 bg-primary text-primary-foreground">
        <span className="text-sm font-black tracking-[0.2em]">A</span>
        <span className="absolute bottom-1.5 right-1.5 h-1.5 w-1.5 rounded-full bg-primary-foreground/75" />
      </div>

      {!compact && (
        <div className="min-w-0">
          <div className="text-sm font-black tracking-[0.28em] text-foreground">
            {APP_NAME}
          </div>
          {subtitle && (
            <div className="text-[11px] font-medium uppercase tracking-[0.16em] text-muted-foreground">
              {subtitle}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
