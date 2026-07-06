import { useState } from 'react';

import { AppShell } from './components/AppShell';
import { type ScreenId } from './nav';
import { OperatorBrief } from './screens/OperatorBrief';
import { PassPlanner } from './screens/PassPlanner';
import { QuickTrack } from './screens/QuickTrack';
import { RFPlanner } from './screens/RFPlanner';
import { RotorControl } from './screens/RotorControl';
import { SatelliteCatalog } from './screens/SatelliteCatalog';
import { Settings } from './screens/Settings';
import { SpaceWeather } from './screens/SpaceWeather';
import { RealtimeProvider } from './stores/realtime';

function App() {
  const [screen, setScreen] = useState<ScreenId>('quick-track');

  return (
    <RealtimeProvider>
      <AppShell active={screen} onNavigate={setScreen}>
        {screen === 'quick-track' && <QuickTrack onNavigate={setScreen} />}
        {screen === 'pass-planner' && <PassPlanner />}
        {screen === 'catalog' && <SatelliteCatalog />}
        {screen === 'rf-planner' && <RFPlanner />}
        {screen === 'space-weather' && <SpaceWeather />}
        {screen === 'rotor-control' && <RotorControl />}
        {screen === 'operator-brief' && <OperatorBrief />}
        {screen === 'settings' && <Settings />}
      </AppShell>
    </RealtimeProvider>
  );
}

export default App;
