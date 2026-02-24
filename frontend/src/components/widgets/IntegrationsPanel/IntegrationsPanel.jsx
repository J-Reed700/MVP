import { useAppContext } from '../../../context/AppContext.jsx';
import './IntegrationsPanel.css';

function IntegrationsPanel() {
  const {
    jiraJql,
    setJiraJql,
    jiraLimit,
    setJiraLimit,
    jiraDefaultOwner,
    setJiraDefaultOwner,
    syncingJira,
    handleJiraSync,
    syncingGong,
    gongLimit,
    setGongLimit,
    gongDefaultOwner,
    setGongDefaultOwner,
    handleGongSync,
    loadingStoryData,
    handleLoadStoryDataset,
    syncResult,
  } = useAppContext();

  return (
    <article className="panel integrations-panel">
      <h2>Integrations</h2>
      <form className="signal-form" onSubmit={handleJiraSync}>
        <label>
          Jira JQL
          <input
            value={jiraJql}
            onChange={(event) => setJiraJql(event.target.value)}
            placeholder="project = CORE ORDER BY updated DESC"
          />
        </label>
        <label>
          Batch Size
          <input
            type="number"
            min="1"
            max="1000"
            value={jiraLimit}
            onChange={(event) => setJiraLimit(event.target.value)}
          />
        </label>
        <label>
          Default Owner (optional)
          <input
            value={jiraDefaultOwner}
            onChange={(event) => setJiraDefaultOwner(event.target.value)}
            placeholder="CS Operations"
          />
        </label>
        <button disabled={syncingJira} type="submit">
          {syncingJira ? 'Syncing Jira...' : 'Sync Jira'}
        </button>
      </form>
      <form className="signal-form" onSubmit={handleGongSync}>
        <label>
          Gong Batch Size
          <input
            type="number"
            min="1"
            max="5000"
            value={gongLimit}
            onChange={(event) => setGongLimit(event.target.value)}
          />
        </label>
        <label>
          Default Owner (optional)
          <input
            value={gongDefaultOwner}
            onChange={(event) => setGongDefaultOwner(event.target.value)}
            placeholder="CS Operations"
          />
        </label>
        <button disabled={syncingGong} type="submit">
          {syncingGong ? 'Syncing Gong...' : 'Sync Gong'}
        </button>
      </form>
      <button
        className="secondary block-btn"
        type="button"
        onClick={handleLoadStoryDataset}
        disabled={loadingStoryData}
      >
        {loadingStoryData ? 'Loading Story Data...' : 'Load Story Dataset'}
      </button>
      <p className="meta">
        Gong pull endpoint: POST /api/integrations/gong/sync · Webhook endpoint: POST
        /api/integrations/gong/webhook
      </p>
      {syncResult && <p className="meta success">{syncResult}</p>}
    </article>
  );
}

export default IntegrationsPanel;
