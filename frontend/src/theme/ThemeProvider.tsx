import { useEffect, useMemo, useState, type ReactNode } from 'react';

import { ThemeContext, type Theme, type ThemeState } from './ThemeContext';

const STORAGE_KEY = 'skycomet.theme';
const THEMES: readonly Theme[] = ['calm', 'paper', 'fog', 'dark', 'midnight', 'console'];

function readStoredTheme(): Theme {
  const stored = localStorage.getItem(STORAGE_KEY);
  return stored && THEMES.includes(stored as Theme) ? (stored as Theme) : 'calm';
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [theme, setTheme] = useState<Theme>(readStoredTheme);

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem(STORAGE_KEY, theme);
  }, [theme]);

  const value = useMemo<ThemeState>(() => ({ theme, setTheme }), [theme]);

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}
