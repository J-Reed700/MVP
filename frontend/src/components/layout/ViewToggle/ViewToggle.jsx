import { useAppContext } from '../../../context/AppContext.jsx';
import './ViewToggle.css';

function ViewToggle() {
  const { activeScreen, setActiveScreen } = useAppContext();

  return (
    <div className="view-toggle-row view-toggle-block">
      <button
        type="button"
        className={activeScreen === 'command_center' ? 'view-toggle active' : 'view-toggle'}
        onClick={() => setActiveScreen('command_center')}
      >
        Command Center
      </button>
      <button
        type="button"
        className={activeScreen === 'settings' ? 'view-toggle active' : 'view-toggle'}
        onClick={() => setActiveScreen('settings')}
      >
        Settings
      </button>
    </div>
  );
}

export default ViewToggle;
