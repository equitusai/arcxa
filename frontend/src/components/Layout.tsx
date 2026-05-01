import React from 'react';
import { Outlet, Link, useLocation } from 'react-router-dom';
import { motion, AnimatePresence } from 'framer-motion';
import { useAppStore } from '@/stores/app';
import { AppLegalFooter } from '@/components/AppLegalFooter';
import { BrandMark } from '@/components/BrandMark';
import { Button } from '@/components/ui/button';
import {
  LayoutDashboard,
  Table2,
  Database,
  Box,
  Network,
  Combine,
  Workflow,
  FileCode,
  ShieldCheck,
  Fingerprint,
  Settings as SettingsIcon,
  Moon,
  Sun,
  Menu,
  X,
  Bell,
  Search,
  FolderOpen,
  BookOpen,
  Files,
  ChevronLeft,
  ChevronRight,
} from 'lucide-react';

// Grouped navigation structure
const navigationGroups = [
  {
    name: 'Dashboard',
    items: [
      { name: 'Dashboard', href: '/', icon: LayoutDashboard },
    ],
  },
  {
    name: 'Data Management',
    items: [
      { name: 'Data Catalogue', href: '/data-catalogue', icon: Database },
      { name: 'Data Sources', href: '/datasources', icon: Database },
      { name: 'File Library', href: '/file-library', icon: Files },
      { name: 'Datasets', href: '/catalogue', icon: FolderOpen },
      { name: 'Entities', href: '/entities', icon: Table2 },
      { name: 'Ontologies', href: '/ontologies', icon: BookOpen },
    ],
  },
  {
    name: 'Operations',
    items: [
      { name: 'Fusion', href: '/fusion', icon: Combine },
      { name: 'Workflows', href: '/workflows', icon: Workflow },
      { name: 'Lineage', href: '/lineage', icon: Network },
    ],
  },
  {
    name: 'Systems-of-Systems',
    items: [
      { name: 'SoS Validation', href: '/sos-validation', icon: ShieldCheck },
    ],
  },
  {
    name: 'Migration Intelligence',
    items: [
      { name: 'Evidence Graph', href: '/migration-evidence', icon: Fingerprint },
    ],
  },
  {
    name: 'Advanced',
    items: [
      { name: 'Models', href: '/models', icon: Box },
      { name: 'SPARQL', href: '/sparql', icon: FileCode },
    ],
  },
  {
    name: 'Settings',
    items: [
      { name: 'Settings', href: '/settings', icon: SettingsIcon },
    ],
  },
];

export function Layout() {
  const location = useLocation();
  const { theme, toggleTheme, sidebarOpen, toggleSidebar, sidebarCollapsed, toggleSidebarCollapsed } = useAppStore();

  return (
    <div className="flex h-screen bg-background">
      {/* Oracle Redwood Sidebar - Traditional, flat navigation */}
      <AnimatePresence mode="wait">
        {sidebarOpen && (
          <motion.aside
            initial={{ opacity: 0 }}
            animate={{
              opacity: 1,
              width: sidebarCollapsed ? '64px' : '240px',
            }}
            exit={{ opacity: 0 }}
            transition={{ duration: 0.2, ease: 'easeInOut' }}
            className="fixed inset-y-0 left-0 z-50 bg-background border-r border-border lg:relative lg:z-0"
          >
            <div className="flex h-full flex-col">
              {/* Logo Header - Oracle style */}
              <div className="flex h-14 items-center justify-between px-4 border-b border-border bg-background-secondary">
                <BrandMark
                  compact={sidebarCollapsed}
                  subtitle={sidebarCollapsed ? undefined : 'Operations Console'}
                />

                <div className="flex items-center gap-2 ml-auto">
                  {/* Desktop collapse button */}
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={toggleSidebarCollapsed}
                    className="hidden lg:flex h-8 w-8 hover:bg-background-tertiary"
                    title={sidebarCollapsed ? 'Expand sidebar' : 'Collapse sidebar'}
                  >
                    {sidebarCollapsed ? (
                      <ChevronRight className="h-4 w-4 text-foreground-secondary" />
                    ) : (
                      <ChevronLeft className="h-4 w-4 text-foreground-secondary" />
                    )}
                  </Button>

                  {/* Mobile close button */}
                  <Button
                    variant="ghost"
                    size="icon"
                    onClick={toggleSidebar}
                    className="lg:hidden h-8 w-8 hover:bg-background-tertiary"
                  >
                    <X className="h-4 w-4 text-foreground-secondary" />
                  </Button>
                </div>
              </div>

              {/* Navigation - Grouped sections */}
              <nav className="flex-1 px-2 py-3 overflow-y-auto">
                <div className="space-y-4">
                  {navigationGroups.map((group) => (
                    <div key={group.name}>
                      {/* Group header - only show for non-Dashboard groups and when not collapsed */}
                      {!sidebarCollapsed && group.name !== 'Dashboard' && (
                        <div className="px-3 mb-2">
                          <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider">
                            {group.name}
                          </h3>
                        </div>
                      )}

                      {/* Group items */}
                      <div className="space-y-1">
                        {group.items.map((item) => {
                          const isActive = location.pathname === item.href;
                          return (
                            <Link
                              key={item.name}
                              to={item.href}
                              onClick={() => {
                                if (window.innerWidth < 1024) {
                                  toggleSidebar();
                                }
                              }}
                              title={sidebarCollapsed ? item.name : undefined}
                              className={`
                                group flex items-center rounded-sm text-sm font-medium transition-colors duration-150
                                ${sidebarCollapsed ? 'justify-center px-3 py-2.5' : 'gap-3 px-3 py-2.5'}
                                ${
                                  isActive
                                    ? 'bg-background-tertiary text-foreground border-l-4 border-primary pl-2'
                                    : 'text-foreground hover:bg-background-secondary border-l-4 border-transparent pl-2'
                                }
                              `}
                            >
                              <item.icon
                                className={`h-4 w-4 transition-colors ${
                                  isActive ? 'text-primary' : 'text-foreground-muted group-hover:text-foreground'
                                } ${sidebarCollapsed ? 'flex-shrink-0' : ''}`}
                              />
                              {!sidebarCollapsed && <span className="flex-1">{item.name}</span>}
                            </Link>
                          );
                        })}
                      </div>
                    </div>
                  ))}
                </div>
              </nav>

              {/* Footer - Oracle style */}
              <div className="border-t border-border px-4 py-3 bg-background-secondary">
                {sidebarCollapsed ? (
                  <div className="flex justify-center">
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={toggleTheme}
                      className="h-8 w-8 hover:bg-background-tertiary transition-colors"
                      title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
                    >
                      {theme === 'dark' ? (
                        <Sun className="h-4 w-4 text-foreground-secondary" />
                      ) : (
                        <Moon className="h-4 w-4 text-foreground-secondary" />
                      )}
                    </Button>
                  </div>
                ) : (
                  <div className="flex items-center justify-between">
                    <div className="flex flex-col">
                      <span className="text-xs text-muted-foreground font-semibold">VERSION</span>
                      <span className="text-xs font-mono text-foreground">v1.0.0</span>
                    </div>
                    <Button
                      variant="ghost"
                      size="icon"
                      onClick={toggleTheme}
                      className="h-8 w-8 hover:bg-background-tertiary transition-colors"
                      title={theme === 'dark' ? 'Switch to light mode' : 'Switch to dark mode'}
                    >
                      {theme === 'dark' ? (
                        <Sun className="h-4 w-4 text-foreground-secondary" />
                      ) : (
                        <Moon className="h-4 w-4 text-foreground-secondary" />
                      )}
                    </Button>
                  </div>
                )}
              </div>
            </div>
          </motion.aside>
        )}
      </AnimatePresence>

      {/* Backdrop for mobile */}
      <AnimatePresence>
        {sidebarOpen && (
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 0.4 }}
            exit={{ opacity: 0 }}
            onClick={toggleSidebar}
            className="fixed inset-0 z-40 bg-black lg:hidden"
          />
        )}
      </AnimatePresence>

      {/* Main content */}
      <div className="flex flex-1 flex-col overflow-hidden">
        {/* Oracle Redwood Header - Toolbar pattern */}
        <header className="flex h-14 items-center justify-between border-b border-border bg-background-secondary px-4 shrink-0">
          <div className="flex items-center space-x-4">
            <Button
              variant="ghost"
              size="icon"
              onClick={toggleSidebar}
              className={`hover:bg-background-tertiary transition-colors h-8 w-8 ${sidebarOpen ? 'lg:hidden' : ''}`}
              title={sidebarOpen ? 'Close sidebar' : 'Open sidebar'}
            >
              <Menu className="h-5 w-5" />
            </Button>

            {/* Search - Oracle input style */}
            <div className="relative hidden md:block">
              <Search className="absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground pointer-events-none" />
              <input
                type="text"
                placeholder="Search data sources, datasets, workflows..."
                className="h-9 w-80 rounded-sm border border-border bg-background pl-9 pr-3 text-sm text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-accent focus:border-accent transition-colors"
              />
            </div>
          </div>

          {/* Right side - Oracle toolbar actions */}
          <div className="flex items-center space-x-3">
            <Button
              variant="ghost"
              size="icon"
              className="h-9 w-9 hover:bg-background-tertiary transition-colors relative"
              title="Notifications"
            >
              <Bell className="h-5 w-5" />
              <span className="absolute top-2 right-2 h-2 w-2 rounded-full bg-error border border-background-secondary" />
            </Button>

            <div className="h-9 w-9 rounded-sm bg-primary flex items-center justify-center cursor-pointer border border-primary" title="User profile">
              <span className="text-white text-sm font-bold">U</span>
            </div>
          </div>
        </header>

        {/* Oracle Redwood page content - Clean background */}
        <main className="flex-1 overflow-auto bg-background-secondary">
          <div className="mx-auto flex min-h-full max-w-[1600px] flex-col px-6 py-5">
            <AnimatePresence mode="wait">
              <motion.div
                key={location.pathname}
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.15 }}
                className="flex-1"
              >
                <Outlet />
              </motion.div>
            </AnimatePresence>
            <AppLegalFooter centered className="mt-8 pb-1" />
          </div>
        </main>
      </div>
    </div>
  );
}
