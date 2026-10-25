import React from 'react';
import { createRoot } from 'react-dom/client';
import { AppProvider } from './store';
import { App } from './App';
import './styles/theme.css';
import './styles/ui.css';

createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <AppProvider>
      <App />
    </AppProvider>
  </React.StrictMode>,
);
