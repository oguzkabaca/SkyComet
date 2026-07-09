export type ScreenId =
  | 'quick-track'
  | 'pass-planner'
  | 'satellite-passes'
  | 'rf-planner'
  | 'catalog'
  | 'space-weather'
  | 'rotor-control'
  | 'operator-brief'
  | 'settings';

export type IconKey =
  | 'quick-track'
  | 'pass-planner'
  | 'rf'
  | 'rotor'
  | 'brief'
  | 'catalog'
  | 'space-weather'
  | 'settings';

export interface NavItem {
  label: string;
  icon: IconKey;
  /** Present → routable screen; absent → roadmap placeholder (K7). */
  screen?: ScreenId;
  badge?: { text: string; kind: 'soon' | 'next' };
  disabled?: boolean;
}

export interface NavGroup {
  title: string;
  items: NavItem[];
}

export const NAV_GROUPS: NavGroup[] = [
  {
    title: 'Tracking',
    items: [{ label: 'Quick track', icon: 'quick-track', screen: 'quick-track' }],
  },
  {
    title: 'Planning',
    items: [
      { label: 'Pass planner', icon: 'pass-planner', screen: 'pass-planner' },
      { label: 'Satellite passes', icon: 'pass-planner', screen: 'satellite-passes' },
    ],
  },
  {
    title: 'RF',
    items: [{ label: 'RF planner', icon: 'rf', screen: 'rf-planner' }],
  },
  {
    title: 'Operations',
    items: [
      { label: 'Rotor control', icon: 'rotor', screen: 'rotor-control' },
      { label: 'Operator brief', icon: 'brief', screen: 'operator-brief' },
    ],
  },
  {
    title: 'System',
    items: [
      { label: 'Catalog', icon: 'catalog', screen: 'catalog' },
      { label: 'Space weather', icon: 'space-weather', screen: 'space-weather' },
      { label: 'Settings', icon: 'settings', screen: 'settings' },
    ],
  },
];

export function findCrumbs(active: ScreenId): { group: string; leaf: string } {
  for (const group of NAV_GROUPS) {
    for (const item of group.items) {
      if (item.screen === active) {
        return { group: group.title, leaf: item.label };
      }
    }
  }
  return { group: '', leaf: '' };
}
