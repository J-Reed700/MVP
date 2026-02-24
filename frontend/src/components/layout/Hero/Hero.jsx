import { useAppContext } from '../../../context/AppContext.jsx';
import './Hero.css';

function Hero() {
  const { lastRefreshedAt, formatDate, loadData, loading } = useAppContext();

  return (
    <header className="hero hero-block">
      <div>
        <p className="eyebrow">Customer Revenue Operations</p>
        <h1>SignalOps Command Center</h1>
        <p className="hero-copy">
          Run weekly risk and growth operations from one screen: see impact, decide ownership, and
          execute next actions.
        </p>
      </div>
      <div className="hero-meta">
        <span>Refresh cadence: on demand</span>
        <strong>{lastRefreshedAt ? `Last refresh ${formatDate(lastRefreshedAt)}` : 'No refresh yet'}</strong>
        <button className="secondary" onClick={loadData} type="button" disabled={loading}>
          {loading ? 'Refreshing...' : 'Refresh Data'}
        </button>
      </div>
    </header>
  );
}

export default Hero;
