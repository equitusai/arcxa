/**
 * LineageTimeSlider Component
 * Temporal navigation slider for lineage graph
 * Allows users to scrub through time and see lineage evolution
 */

import React, { useState, useMemo } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Slider } from '@/components/ui/slider';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import {
  Play,
  Pause,
  SkipBack,
  SkipForward,
  Calendar,
  Clock,
} from 'lucide-react';
import { cn } from '@/lib/utils';
import { format, formatDistanceToNow } from 'date-fns';

interface LineageTimeSliderProps {
  dateRange: { start: string; end: string };
  selectedTime?: string;
  onTimeChange: (timestamp: string) => void;
  totalEvents: number;
  className?: string;
}

export function LineageTimeSlider({
  dateRange,
  selectedTime,
  onTimeChange,
  totalEvents,
  className,
}: LineageTimeSliderProps) {
  const [isPlaying, setIsPlaying] = useState(false);
  const [playbackSpeed, setPlaybackSpeed] = useState<1 | 2 | 4>(1);

  // Calculate time points
  const { startMs, endMs, durationMs, currentMs } = useMemo(() => {
    const start = new Date(dateRange.start).getTime();
    const end = new Date(dateRange.end).getTime();
    const current = selectedTime ? new Date(selectedTime).getTime() : end;
    return {
      startMs: start,
      endMs: end,
      durationMs: end - start,
      currentMs: current,
    };
  }, [dateRange, selectedTime]);

  // Calculate slider position (0-100)
  const sliderPosition = useMemo(() => {
    if (durationMs === 0) return 100;
    return ((currentMs - startMs) / durationMs) * 100;
  }, [currentMs, startMs, durationMs]);

  // Handle slider change
  const handleSliderChange = (values: number[]) => {
    const position = values[0];
    const newMs = startMs + (durationMs * position) / 100;
    onTimeChange(new Date(newMs).toISOString());
  };

  // Play/Pause animation
  React.useEffect(() => {
    if (!isPlaying) return;

    const interval = setInterval(() => {
      const step = (durationMs * playbackSpeed) / 100; // Move 1% per tick
      const newMs = Math.min(currentMs + step, endMs);

      if (newMs >= endMs) {
        setIsPlaying(false);
        onTimeChange(new Date(endMs).toISOString());
      } else {
        onTimeChange(new Date(newMs).toISOString());
      }
    }, 100); // 100ms per tick = 10fps

    return () => clearInterval(interval);
  }, [isPlaying, currentMs, endMs, startMs, durationMs, playbackSpeed, onTimeChange]);

  // Skip to start
  const handleSkipToStart = () => {
    setIsPlaying(false);
    onTimeChange(new Date(startMs).toISOString());
  };

  // Skip to end
  const handleSkipToEnd = () => {
    setIsPlaying(false);
    onTimeChange(new Date(endMs).toISOString());
  };

  // Toggle play/pause
  const handleTogglePlay = () => {
    if (currentMs >= endMs) {
      // If at end, restart from beginning
      onTimeChange(new Date(startMs).toISOString());
      setIsPlaying(true);
    } else {
      setIsPlaying(!isPlaying);
    }
  };

  // Cycle playback speed
  const handleCycleSpeed = () => {
    setPlaybackSpeed((prev) => {
      if (prev === 1) return 2;
      if (prev === 2) return 4;
      return 1;
    });
  };

  // Format current time
  const currentTimeFormatted = format(new Date(currentMs), 'PPp');
  const relativeTime = formatDistanceToNow(new Date(currentMs), { addSuffix: true });

  return (
    <Card className={cn('', className)}>
      <CardContent className="py-3 px-4">
        <div className="space-y-3">
          {/* Time Display */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-2">
              <Clock className="h-4 w-4 text-muted-foreground" />
              <div className="flex flex-col">
                <span className="text-xs font-semibold text-foreground">
                  {currentTimeFormatted}
                </span>
                <span className="text-[10px] text-muted-foreground">{relativeTime}</span>
              </div>
            </div>
            <div className="flex items-center gap-2">
              <Badge variant="secondary" className="text-[10px] px-1.5 py-0 h-5">
                {totalEvents} events
              </Badge>
              {isPlaying && (
                <Badge variant="default" className="text-[10px] px-1.5 py-0 h-5 animate-pulse">
                  Playing
                </Badge>
              )}
            </div>
          </div>

          {/* Timeline Slider */}
          <div className="flex items-center gap-3">
            <span className="text-[10px] text-muted-foreground tabular-nums">
              {format(new Date(startMs), 'MM/dd/yy')}
            </span>
            <div className="flex-1">
              <Slider
                min={0}
                max={100}
                step={0.1}
                value={[sliderPosition]}
                onValueChange={handleSliderChange}
                className="w-full"
                disabled={isPlaying}
              />
            </div>
            <span className="text-[10px] text-muted-foreground tabular-nums">
              {format(new Date(endMs), 'MM/dd/yy')}
            </span>
          </div>

          {/* Playback Controls */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-1">
              <Button
                variant="ghost"
                size="sm"
                onClick={handleSkipToStart}
                disabled={isPlaying || currentMs <= startMs}
                className="h-7 w-7 p-0"
                title="Skip to Start"
              >
                <SkipBack className="h-3.5 w-3.5" />
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleTogglePlay}
                className="h-7 w-7 p-0"
                title={isPlaying ? 'Pause' : 'Play'}
              >
                {isPlaying ? (
                  <Pause className="h-3.5 w-3.5" />
                ) : (
                  <Play className="h-3.5 w-3.5" />
                )}
              </Button>
              <Button
                variant="ghost"
                size="sm"
                onClick={handleSkipToEnd}
                disabled={isPlaying || currentMs >= endMs}
                className="h-7 w-7 p-0"
                title="Skip to End"
              >
                <SkipForward className="h-3.5 w-3.5" />
              </Button>
            </div>

            <div className="flex items-center gap-2">
              <Button
                variant="outline"
                size="sm"
                onClick={handleCycleSpeed}
                className="h-7 px-2 text-xs"
                title="Playback Speed"
              >
                {playbackSpeed}x
              </Button>
              <Badge variant="outline" className="text-[10px] px-1.5 py-0 h-5">
                <Calendar className="h-3 w-3 mr-1" />
                {format(new Date(durationMs), 'd')}d range
              </Badge>
            </div>
          </div>

          {/* Progress Bar */}
          <div className="h-1 bg-muted rounded-full overflow-hidden">
            <div
              className={cn(
                'h-full bg-accent rounded-full transition-all',
                isPlaying && 'animate-pulse'
              )}
              style={{ width: `${sliderPosition}%` }}
            />
          </div>
        </div>
      </CardContent>
    </Card>
  );
}
