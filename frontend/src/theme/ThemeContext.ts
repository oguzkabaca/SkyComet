import { createContext } from 'react';

export type Theme = 'calm' | 'paper' | 'fog' | 'dark' | 'midnight' | 'console';

export interface ThemeState {
  theme: Theme;
  setTheme: (theme: Theme) => void;
}

export const ThemeContext = createContext<ThemeState | null>(null);
