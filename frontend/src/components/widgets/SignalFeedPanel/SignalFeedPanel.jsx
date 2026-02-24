import { useAppContext } from '../../../context/AppContext.jsx';
import './SignalFeedPanel.css';

function SignalFeedPanel() {
  const {
    filteredSignals,
    query,
    setQuery,
    sourceFilter,
    setSourceFilter,
    sourceOptions,
    ownerFilter,
    setOwnerFilter,
    hideSynthetic,
    setHideSynthetic,
    loading,
    pagedSignals,
    formatRelative,
    formatDate,
    page,
    totalPages,
    setPage,
  } = useAppContext();

  return (
    <article className="panel signal-feed-panel">
      <div className="section-head">
        <h2>Signal Feed</h2>
        <p className="meta">{filteredSignals.length} matching signals</p>
      </div>

      <div className="filter-grid">
        <label>
          Search
          <input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder="account name, owner, tag, summary"
          />
        </label>

        <label>
          Source
          <select value={sourceFilter} onChange={(event) => setSourceFilter(event.target.value)}>
            {sourceOptions.map((source) => (
              <option key={source} value={source}>
                {source === 'all' ? 'All Sources' : source}
              </option>
            ))}
          </select>
        </label>

        <label>
          Ownership
          <select value={ownerFilter} onChange={(event) => setOwnerFilter(event.target.value)}>
            <option value="all">All</option>
            <option value="assigned">Assigned</option>
            <option value="unassigned">Unassigned</option>
          </select>
        </label>

        <label className="filter-toggle">
          <span>Focus Mode</span>
          <button
            type="button"
            className={hideSynthetic ? 'secondary active-toggle' : 'secondary'}
            onClick={() => setHideSynthetic((value) => !value)}
          >
            {hideSynthetic ? 'On: hide synthetic noise' : 'Off: include synthetic records'}
          </button>
        </label>
      </div>

      {loading ? (
        <p>Loading signals...</p>
      ) : pagedSignals.length === 0 ? (
        <p>No signals match your current filters.</p>
      ) : (
        <ul className="feed-list">
          {pagedSignals.map((signal) => (
            <li key={signal.id}>
              <div className="feed-title-row">
                <h3>{signal.title}</h3>
                <span className="recency">{formatRelative(signal.updated_at)}</span>
              </div>
              <p>{signal.summary}</p>
              <p className="meta">
                Owner: {signal.owner ?? 'Unassigned'} · Sources:{' '}
                {(signal.source_systems ?? []).join(', ')} · Updated {formatDate(signal.updated_at)}
              </p>
              {(signal.tags ?? []).length > 0 && (
                <div className="tag-row">
                  {signal.tags.slice(0, 5).map((tag) => (
                    <span key={`${signal.id}-${tag}`} className="tag-chip">
                      {tag}
                    </span>
                  ))}
                </div>
              )}
            </li>
          ))}
        </ul>
      )}

      <div className="pager-row">
        <button
          className="secondary"
          onClick={() => setPage((value) => Math.max(1, value - 1))}
          type="button"
          disabled={page <= 1}
        >
          Prev
        </button>
        <span>
          Page {page} of {totalPages}
        </span>
        <button
          className="secondary"
          onClick={() => setPage((value) => Math.min(totalPages, value + 1))}
          type="button"
          disabled={page >= totalPages}
        >
          Next
        </button>
      </div>
    </article>
  );
}

export default SignalFeedPanel;
