import { useAppContext } from '../../../context/AppContext.jsx';
import './KpiGrid.css';

function KpiGrid() {
  const {
    signals,
    dueSoonCount,
    highPriorityCount,
    ownershipCoverage,
    unassignedCount,
    staleCount,
  } = useAppContext();

  return (
    <section className="kpi-grid kpi-grid-block">
      <article className="kpi-card">
        <p>Active Records</p>
        <strong>{signals.length}</strong>
        <span>Signals currently in operating view</span>
      </article>
      <article className="kpi-card">
        <p>Actions Due Soon</p>
        <strong>{dueSoonCount}</strong>
        <span>{highPriorityCount} high-priority actions in queue</span>
      </article>
      <article className="kpi-card">
        <p>Assigned Ownership</p>
        <strong>{ownershipCoverage}%</strong>
        <span>{unassignedCount} records need an accountable owner</span>
      </article>
      <article className="kpi-card">
        <p>Needs Refresh</p>
        <strong>{staleCount}</strong>
        <span>Records older than 30 days</span>
      </article>
    </section>
  );
}

export default KpiGrid;
