import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App.tsx'
import './index.css'
import './utilities.css'  // CRITICAL: Custom utilities MUST be imported after Tailwind

// Initialize theme BEFORE React renders to prevent FOUC
// This ensures the correct CSS variables are applied from the start
const initializeTheme = () => {
  try {
    const stored = localStorage.getItem('graphica-app-store');
    if (stored) {
      const data = JSON.parse(stored);
      const theme = data?.state?.theme || 'light';

      if (theme === 'dark') {
        document.documentElement.classList.add('dark');
      } else {
        document.documentElement.classList.remove('dark');
      }
    } else {
      // Default to light mode
      document.documentElement.classList.remove('dark');
    }
  } catch (e) {
    // If anything fails, default to light mode
    document.documentElement.classList.remove('dark');
  }
};

// Run theme initialization immediately
initializeTheme();

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
