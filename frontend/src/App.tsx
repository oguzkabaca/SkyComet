import { useCallback, useState } from 'react';

import { AppShell } from './components/AppShell';
import {
  isOperationIntentV1,
  type OperationIntentV1,
} from './lib/operationContext';
import { type ScreenId } from './nav';
import { OperatorBrief } from './screens/OperatorBrief';
import { PassPlanner } from './screens/PassPlanner';
import { QuickTrack } from './screens/QuickTrack';
import { RFPlanner } from './screens/RFPlanner';
import { RotorControl } from './screens/RotorControl';
import { SatelliteCatalog } from './screens/SatelliteCatalog';
import { SatellitePasses } from './screens/SatellitePasses';
import { Settings } from './screens/Settings';
import { SpaceWeather } from './screens/SpaceWeather';
import { RealtimeProvider } from './stores/realtime';

function App() {
  const [screen, setScreen] = useState<ScreenId>('quick-track');
  const [operationIntent, setOperationIntent] = useState<OperationIntentV1 | null>(null);

  const openOperation = useCallback((intent: OperationIntentV1) => {
    // Treat even in-memory navigation as an input boundary. Malformed context
    // must never silently degrade into a different satellite/pass.
    if (!isOperationIntentV1(intent)) return;
    setOperationIntent(intent);
    setScreen(intent.destination);
  }, []);

  const consumeOperation = useCallback(() => setOperationIntent(null), []);

  const navigate = useCallback((next: ScreenId) => {
    setOperationIntent(null);
    setScreen(next);
  }, []);

  return (
    <RealtimeProvider>
      <AppShell active={screen} onNavigate={navigate}>
        {screen === 'quick-track' && (
          <QuickTrack
            onNavigate={navigate}
            operationIntent={
              operationIntent?.destination === 'quick-track' ? operationIntent : null
            }
            onConsumeOperation={consumeOperation}
          />
        )}
        {screen === 'pass-planner' && <PassPlanner onOpenOperation={openOperation} />}
        {screen === 'satellite-passes' && <SatellitePasses />}
        {screen === 'catalog' && <SatelliteCatalog />}
        {screen === 'rf-planner' && (
          <RFPlanner
            operationIntent={
              operationIntent?.destination === 'rf-planner' ? operationIntent : null
            }
            onConsumeOperation={consumeOperation}
            onOpenOperation={openOperation}
          />
        )}
        {screen === 'space-weather' && <SpaceWeather />}
        {screen === 'rotor-control' && <RotorControl />}
        {screen === 'operator-brief' && <OperatorBrief />}
        {screen === 'settings' && <Settings />}
      </AppShell>
    </RealtimeProvider>
  );
}

export default App;
