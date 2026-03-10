import { useQuery } from '@tanstack/react-query';
import * as healthApi from '@/api/health';

export function useHealth() {
  return useQuery({
    queryKey: ['health'],
    queryFn: () => healthApi.getHealth(),
    staleTime: 30 * 1000,
    refetchInterval: 60 * 1000, // Refetch every minute
  });
}

export function useLiveness() {
  return useQuery({
    queryKey: ['health', 'live'],
    queryFn: () => healthApi.getLiveness(),
    staleTime: 30 * 1000,
    refetchInterval: 60 * 1000,
  });
}

export function useReadiness() {
  return useQuery({
    queryKey: ['health', 'ready'],
    queryFn: () => healthApi.getReadiness(),
    staleTime: 30 * 1000,
    refetchInterval: 60 * 1000,
  });
}

export function useStorageHealth() {
  return useQuery({
    queryKey: ['health', 'storage'],
    queryFn: () => healthApi.getStorageHealth(),
    staleTime: 1 * 60 * 1000,
  });
}

export function useMetrics() {
  return useQuery({
    queryKey: ['metrics'],
    queryFn: () => healthApi.getMetrics(),
    staleTime: 30 * 1000,
    refetchInterval: 60 * 1000,
  });
}
