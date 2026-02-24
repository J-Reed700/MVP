import { useAppContext } from '../../../context/AppContext.jsx';
import './ExecutiveBrief.css';

function ExecutiveBrief() {
  const { topInsight, syntheticCount, hideSynthetic, actionResult } = useAppContext();

  return (
    <section className="brief-grid brief-grid-block">
      <article className="panel brief-card">
        <h2>Weekly Operating Brief</h2>
        {topInsight ? (
          <>
            <p className="meta">Top business priority</p>
            <p className="brief-headline">{topInsight.title}</p>
            <p>{topInsight.recommendation}</p>
          </>
        ) : (
          <p>No urgent actions detected.</p>
        )}
        <p className="meta">
          Demo records loaded: {syntheticCount} · Feed currently{' '}
          {hideSynthetic ? 'hides synthetic records' : 'shows all records'}
        </p>
        {actionResult && <p className="meta success">{actionResult}</p>}
      </article>
    </section>
  );
}

export default ExecutiveBrief;
