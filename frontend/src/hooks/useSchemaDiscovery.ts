/**
 * React Query hooks for datasource-backed schema discovery.
 */

import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { useState, useEffect, useCallback, useRef } from 'react';
import axios from 'axios';
import type {
  DiscoveryRequest,
  DiscoveryProgress,
  DiscoveryResult,
} from '@/types/discovery';
import { toast } from 'sonner';

const API_BASE = '/api/v1';
const DISCOVERY_PAGE_SIZE = 250;

interface DiscoveryResultPage extends DiscoveryResult {}

function getErrorMessage(error: unknown): string {
  const apiError = error as {
    message?: string;
    response?: {
      data?: {
        details?: string;
        error?: string;
      };
    };
  };

  return (
    apiError.response?.data?.details ||
    apiError.response?.data?.error ||
    apiError.message ||
    'Request failed'
  );
}

async function startDiscovery(request: DiscoveryRequest): Promise<{ discovery_id: string }> {
  const response = await axios.post(
    `${API_BASE}/datasources/${request.datasource_id}/discover`,
    request.options
  );

  return response.data;
}

async function getDiscoveryProgress(
  datasource_id: string,
  discovery_id: string
): Promise<DiscoveryProgress> {
  const response = await axios.get(`${API_BASE}/datasources/${datasource_id}/discovery/progress`, {
    params: { discovery_id },
  });
  return response.data;
}

async function getDiscoveryResultPage(
  datasource_id: string,
  discovery_id: string,
  offset: number
): Promise<DiscoveryResultPage> {
  const response = await axios.get(`${API_BASE}/datasources/${datasource_id}/discovery/result`, {
    params: {
      discovery_id,
      limit: DISCOVERY_PAGE_SIZE,
      offset,
    },
  });
  return response.data;
}

async function getDiscoveryResult(
  datasource_id: string,
  discovery_id: string
): Promise<DiscoveryResult> {
  const firstPage = await getDiscoveryResultPage(datasource_id, discovery_id, 0);

  if (firstPage.tables.length >= firstPage.total) {
    return firstPage;
  }

  const tables = [...firstPage.tables];
  let offset = tables.length;

  while (offset < firstPage.total) {
    const page = await getDiscoveryResultPage(datasource_id, discovery_id, offset);
    if (page.tables.length === 0) {
      break;
    }

    tables.push(...page.tables);
    offset += page.tables.length;
  }

  return {
    ...firstPage,
    tables,
    page: 0,
    page_size: tables.length,
  };
}

async function cancelDiscovery(datasource_id: string, discovery_id: string): Promise<void> {
  await axios.delete(`${API_BASE}/datasources/${datasource_id}/discovery`, {
    params: { discovery_id },
  });
}

export function useStartDiscovery() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: startDiscovery,
    onSuccess: (data, variables) => {
      toast.success('Schema discovery started', {
        description: `Discovery ID: ${data.discovery_id}`,
      });
      queryClient.invalidateQueries({ queryKey: ['datasources', variables.datasource_id] });
    },
    onError: (error: unknown) => {
      toast.error('Failed to start discovery', {
        description: getErrorMessage(error),
      });
    },
  });
}

export function useDiscoveryProgress(
  datasource_id: string | undefined,
  discovery_id: string | undefined,
  options?: {
    enabled?: boolean;
    refetchInterval?: number;
  }
) {
  return useQuery({
    queryKey: ['discovery-progress', datasource_id, discovery_id],
    queryFn: () => {
      if (!datasource_id || !discovery_id) {
        throw new Error('Missing datasource_id or discovery_id');
      }
      return getDiscoveryProgress(datasource_id, discovery_id);
    },
    enabled: Boolean(datasource_id && discovery_id && (options?.enabled ?? true)),
    refetchInterval: options?.refetchInterval || 2000,
    refetchIntervalInBackground: true,
  });
}

export function useDiscoveryResult(
  datasource_id: string | undefined,
  discovery_id: string | undefined,
  options?: {
    enabled?: boolean;
  }
) {
  return useQuery({
    queryKey: ['discovery-result', datasource_id, discovery_id],
    queryFn: () => {
      if (!datasource_id || !discovery_id) {
        throw new Error('Missing datasource_id or discovery_id');
      }
      return getDiscoveryResult(datasource_id, discovery_id);
    },
    enabled: Boolean(datasource_id && discovery_id && (options?.enabled ?? true)),
    staleTime: 5 * 60 * 1000,
  });
}

export function useCancelDiscovery() {
  const queryClient = useQueryClient();

  return useMutation({
    mutationFn: ({ datasource_id, discovery_id }: { datasource_id: string; discovery_id: string }) =>
      cancelDiscovery(datasource_id, discovery_id),
    onSuccess: (_, variables) => {
      toast.success('Discovery cancelled');
      queryClient.invalidateQueries({
        queryKey: ['discovery-progress', variables.datasource_id, variables.discovery_id],
      });
    },
    onError: (error: unknown) => {
      toast.error('Failed to cancel discovery', {
        description: getErrorMessage(error),
      });
    },
  });
}

export interface UseDiscoveryStreamOptions {
  onProgress?: (progress: DiscoveryProgress) => void;
  onComplete?: (result: DiscoveryResult) => void;
  onError?: (error: string) => void;
}

export function useDiscoveryStream(
  datasource_id: string | undefined,
  discovery_id: string | undefined,
  options?: UseDiscoveryStreamOptions
) {
  const [isConnected, setIsConnected] = useState(false);
  const [progress, setProgress] = useState<DiscoveryProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const eventSourceRef = useRef<EventSource | null>(null);
  const completedRef = useRef(false);
  const optionsRef = useRef(options);
  const queryClient = useQueryClient();

  useEffect(() => {
    optionsRef.current = options;
  }, [options]);

  const disconnect = useCallback(() => {
    if (eventSourceRef.current) {
      eventSourceRef.current.close();
      eventSourceRef.current = null;
    }
    setIsConnected(false);
  }, []);

  const connect = useCallback(() => {
    if (!datasource_id || !discovery_id) {
      return;
    }

    completedRef.current = false;
    setError(null);
    disconnect();

    const params = new URLSearchParams({ discovery_id });
    const eventSource = new EventSource(
      `${API_BASE}/datasources/${datasource_id}/discovery/stream?${params.toString()}`
    );
    eventSourceRef.current = eventSource;

    eventSource.onopen = () => {
      setIsConnected(true);
      setError(null);
    };

    eventSource.addEventListener('progress', (event: MessageEvent) => {
      void (async () => {
        try {
          const progressData = JSON.parse(event.data) as DiscoveryProgress;
          setProgress(progressData);
          queryClient.setQueryData(
            ['discovery-progress', datasource_id, discovery_id],
            progressData
          );
          optionsRef.current?.onProgress?.(progressData);

          if (progressData.status === 'completed' && !completedRef.current) {
            completedRef.current = true;
            const result = await getDiscoveryResult(datasource_id, discovery_id);
            queryClient.setQueryData(
              ['discovery-result', datasource_id, discovery_id],
              result
            );
            optionsRef.current?.onComplete?.(result);
            disconnect();
            return;
          }

          if (progressData.status === 'failed' || progressData.status === 'cancelled') {
            const message =
              progressData.errors[0] ||
              (progressData.status === 'cancelled'
                ? 'Discovery was cancelled'
                : 'Discovery failed');
            setError(message);
            optionsRef.current?.onError?.(message);
            disconnect();
          }
        } catch (streamError) {
          const message = getErrorMessage(streamError);
          setError(message);
          optionsRef.current?.onError?.(message);
          disconnect();
        }
      })();
    });

    eventSource.addEventListener('error', (event: MessageEvent) => {
      const message = typeof event.data === 'string' && event.data.length > 0
        ? event.data
        : 'Discovery stream error';
      setError(message);
      optionsRef.current?.onError?.(message);
      disconnect();
    });

    eventSource.onerror = () => {
      if (completedRef.current) {
        return;
      }

      const message = 'Discovery stream disconnected';
      setError(message);
      optionsRef.current?.onError?.(message);
      disconnect();
    };
  }, [datasource_id, discovery_id, disconnect, queryClient]);

  useEffect(() => {
    if (datasource_id && discovery_id) {
      connect();
    }

    return () => {
      disconnect();
    };
  }, [datasource_id, discovery_id, connect, disconnect]);

  return {
    isConnected,
    progress,
    error,
    connect,
    disconnect,
  };
}

export function useDiscoveryMonitor(
  datasource_id: string | undefined,
  discovery_id: string | undefined,
  options?: UseDiscoveryStreamOptions
) {
  const [usePolling, setUsePolling] = useState(false);

  const sseResult = useDiscoveryStream(datasource_id, discovery_id, {
    ...options,
    onError: (message) => {
      setUsePolling(true);
      options?.onError?.(message);
    },
  });

  const pollingResult = useDiscoveryProgress(datasource_id, discovery_id, {
    enabled: usePolling,
    refetchInterval: 2000,
  });

  return {
    progress: sseResult.progress || pollingResult.data || null,
    isConnected: sseResult.isConnected || pollingResult.isFetching,
    error:
      sseResult.error ||
      (pollingResult.error ? getErrorMessage(pollingResult.error) : null),
    isLoading: pollingResult.isLoading,
  };
}
