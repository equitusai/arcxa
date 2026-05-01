import React, { Suspense, lazy } from 'react';
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { Toaster } from 'sonner';
import { Layout } from '@/components/Layout';
import { ProtectedRoute } from '@/components/auth/ProtectedRoute';
import { APP_NAME } from '@/lib/branding';

const Login = lazy(() => import('@/pages/Login').then((module) => ({ default: module.Login })));
const DashboardV2 = lazy(() =>
  import('@/pages/DashboardV2').then((module) => ({ default: module.DashboardV2 }))
);
const Entities = lazy(() =>
  import('@/pages/Entities').then((module) => ({ default: module.Entities }))
);
const Datasources = lazy(() =>
  import('@/pages/DatasourcesV2').then((module) => ({ default: module.DatasourcesV2 }))
);
const Catalogue = lazy(() => import('@/pages/Catalogue'));
const DatasetDetail = lazy(() =>
  import('@/pages/DatasetDetail').then((module) => ({ default: module.DatasetDetail }))
);
const Models = lazy(() => import('@/pages/Models').then((module) => ({ default: module.Models })));
const Lineage = lazy(() => import('@/pages/Lineage').then((module) => ({ default: module.Lineage })));
const Fusion = lazy(() => import('@/pages/Fusion').then((module) => ({ default: module.Fusion })));
const FusionNew = lazy(() =>
  import('@/pages/FusionNew').then((module) => ({ default: module.FusionNew }))
);
const SparqlPlayground = lazy(() =>
  import('@/pages/SparqlPlayground').then((module) => ({ default: module.SparqlPlayground }))
);
const Settings = lazy(() =>
  import('@/pages/Settings').then((module) => ({ default: module.Settings }))
);
const WorkflowDesigner = lazy(() =>
  import('@/pages/WorkflowDesigner').then((module) => ({ default: module.WorkflowDesigner }))
);
const Ontologies = lazy(() =>
  import('@/pages/Ontologies').then((module) => ({ default: module.Ontologies }))
);
const FileLibrary = lazy(() =>
  import('@/pages/FileLibrary').then((module) => ({ default: module.FileLibrary }))
);
const DataCatalogue = lazy(() =>
  import('@/pages/DataCatalogue').then((module) => ({ default: module.DataCatalogue }))
);
const SosValidation = lazy(() =>
  import('@/pages/SosValidation').then((module) => ({ default: module.SosValidation }))
);
const MigrationEvidence = lazy(() =>
  import('@/pages/MigrationEvidence').then((module) => ({ default: module.MigrationEvidence }))
);

// Create a client for React Query
const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 5 * 60 * 1000, // 5 minutes
      gcTime: 10 * 60 * 1000, // 10 minutes (formerly cacheTime)
      refetchOnWindowFocus: false,
      retry: 1,
    },
  },
});

function App() {
  return (
    <QueryClientProvider client={queryClient}>
      <BrowserRouter>
        <Toaster
          position="top-right"
          expand={false}
          richColors
          closeButton
        />
        <Suspense fallback={<RouteFallback />}>
          <Routes>
            {/* Public routes */}
            <Route path="/login" element={<Login />} />

            {/* Protected routes */}
            <Route
              path="/"
              element={
                <ProtectedRoute>
                  <Layout />
                </ProtectedRoute>
              }
            >
              <Route index element={<DashboardV2 />} />
              <Route path="data-catalogue" element={<DataCatalogue />} />
              <Route path="catalogue" element={<Catalogue />} />
              <Route path="catalogue/:datasetId" element={<DatasetDetail />} />
              <Route path="entities" element={<Entities />} />
              <Route path="datasources" element={<Datasources />} />
              <Route path="file-library" element={<FileLibrary />} />
              <Route path="models" element={<Models />} />
              <Route path="lineage" element={<Lineage />} />
              <Route path="fusion" element={<Fusion />} />
              <Route path="fusion-new" element={<FusionNew />} />
              <Route path="workflows" element={<WorkflowDesigner />} />
              <Route path="sos-validation" element={<SosValidation />} />
              <Route path="migration-evidence" element={<MigrationEvidence />} />
              <Route path="ontologies" element={<Ontologies />} />
              <Route path="sparql" element={<SparqlPlayground />} />
              <Route
                path="settings"
                element={
                  <ProtectedRoute requiredRole="Admin">
                    <Settings />
                  </ProtectedRoute>
                }
              />
            </Route>
          </Routes>
        </Suspense>
      </BrowserRouter>
    </QueryClientProvider>
  );
}

function RouteFallback() {
  return (
    <div className="flex min-h-screen items-center justify-center bg-background">
      <div className="text-sm text-muted-foreground">Loading {APP_NAME}...</div>
    </div>
  );
}

export default App
