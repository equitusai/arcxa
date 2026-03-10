import React from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '@/components/ui/card';
import { Badge } from '@/components/ui/badge';
import {
  Database,
  Brain,
  GitMerge,
  AlertCircle,
  CheckCircle,
  Activity as ActivityIcon
} from 'lucide-react';
import { motion } from 'framer-motion';
import { cn } from '@/lib/utils';

interface ActivityItem {
  id: string | number;
  type: 'entity_created' | 'model_deployed' | 'fusion_completed' | 'quality_alert' | 'entity_updated';
  message: string;
  time: string;
  metadata?: {
    count?: number;
    domain?: string;
    modelVersion?: string;
  };
}

interface ActivityFeedProps {
  activities: ActivityItem[];
}

const activityConfig = {
  entity_created: {
    icon: Database,
    color: 'entity' as const,
    bgColor: 'bg-entity/10',
    label: 'Entity Created'
  },
  model_deployed: {
    icon: Brain,
    color: 'model' as const,
    bgColor: 'bg-model/10',
    label: 'Model Deployed'
  },
  fusion_completed: {
    icon: GitMerge,
    color: 'success' as const,
    bgColor: 'bg-success/10',
    label: 'Fusion Complete'
  },
  quality_alert: {
    icon: AlertCircle,
    color: 'warning' as const,
    bgColor: 'bg-warning/10',
    label: 'Quality Alert'
  },
  entity_updated: {
    icon: CheckCircle,
    color: 'entity' as const,
    bgColor: 'bg-entity/10',
    label: 'Entity Updated'
  }
};

export function ActivityFeed({ activities }: ActivityFeedProps) {
  return (
    <Card className="h-full">
      <CardHeader>
        <div className="flex items-center gap-3">
          <div className="p-2 rounded-sm bg-entity/10">
            <ActivityIcon className="h-5 w-5 text-entity" />
          </div>
          <CardTitle>Recent Activity</CardTitle>
        </div>
        <CardDescription>Latest operations in your data platform</CardDescription>
      </CardHeader>
      <CardContent>
        <div className="space-y-2">
          {activities.map((activity, index) => {
            const config = activityConfig[activity.type];
            const Icon = config.icon;

            return (
              <motion.div
                key={activity.id}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                transition={{ duration: 0.15, delay: index * 0.03 }}
                className="flex items-start gap-3 p-3 rounded-sm hover:bg-background-tertiary transition-colors duration-150 border border-border"
              >
                <div className={cn("p-2 rounded-sm shrink-0", config.bgColor)}>
                  <Icon className={cn(
                    "h-4 w-4",
                    config.color === 'entity' && "text-entity",
                    config.color === 'model' && "text-model",
                    config.color === 'success' && "text-success",
                    config.color === 'warning' && "text-warning"
                  )} />
                </div>
                <div className="flex-1 space-y-1 min-w-0">
                  <div className="flex items-start justify-between gap-2">
                    <p className="text-sm text-foreground">{activity.message}</p>
                    <Badge variant={config.color} className="shrink-0">
                      {config.label}
                    </Badge>
                  </div>
                  <p className="text-xs text-muted-foreground font-semibold">{activity.time}</p>
                </div>
              </motion.div>
            );
          })}
        </div>
      </CardContent>
    </Card>
  );
}
