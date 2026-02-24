import Hero from './components/layout/Hero/Hero.jsx';
import ViewToggle from './components/layout/ViewToggle/ViewToggle.jsx';
import CommandCenterScreen from './components/screens/CommandCenterScreen/CommandCenterScreen.jsx';
import SettingsScreen from './components/screens/SettingsScreen/SettingsScreen.jsx';
import { AppProvider, useAppContext } from './context/AppContext.jsx';

function AppShell() {
  const { activeScreen, error } = useAppContext();

  return (
    <main className="app-shell">
      <Hero />
      <ViewToggle />
      {error && <p className="error">{error}</p>}
      {activeScreen === 'settings' ? <SettingsScreen /> : <CommandCenterScreen />}
    </main>
  );
}

function App() {
  return (
    <AppProvider>
      <AppShell />
    </AppProvider>
  );
}

export default App;
