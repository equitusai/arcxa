import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'
import path from 'path'

// https://vitejs.dev/config/
export default defineConfig({
  plugins: [react()],
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  build: {
    rollupOptions: {
      output: {
        manualChunks(id) {
          if (!id.includes('node_modules')) {
            return;
          }

          if (id.includes('@monaco-editor') || id.includes('monaco-editor')) {
            return 'vendor-monaco';
          }

          if (id.includes('recharts') || id.includes('d3-')) {
            return 'vendor-charts';
          }

          if (id.includes('lucide-react')) {
            return 'vendor-icons';
          }

          if (id.includes('framer-motion')) {
            return 'vendor-motion';
          }

          if (id.includes('date-fns')) {
            return 'vendor-date';
          }

          if (id.includes('reactflow')) {
            return 'vendor-reactflow';
          }

          if (id.includes('@radix-ui')) {
            return 'vendor-radix';
          }

          if (id.includes('@tanstack/react-query')) {
            return 'vendor-query';
          }

          if (
            id.includes('/react/') ||
            id.includes('/react-') ||
            id.includes('react-dom') ||
            id.includes('react-is') ||
            id.includes('react-router') ||
            id.includes('react-router-dom') ||
            id.includes('use-sync-external-store') ||
            id.includes('/cookie/') ||
            id.includes('set-cookie-parser') ||
            id.includes('prop-types') ||
            id.includes('scheduler')
          ) {
            return 'vendor-react';
          }

          if (id.includes('n3')) {
            return 'vendor-rdf';
          }

          if (
            id.includes('axios') ||
            id.includes('sonner') ||
            id.includes('zustand') ||
            id.includes('react-window') ||
            id.includes('html2canvas') ||
            id.includes('file-saver') ||
            id.includes('fuse.js')
          ) {
            return 'vendor-utils';
          }
        },
      },
    },
  },
  server: {
    port: 5173,
    host: true,
    proxy: {
      '/openapi.yaml': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
      '/api': {
        target: 'http://localhost:8080',
        changeOrigin: true,
      },
    },
  },
})
