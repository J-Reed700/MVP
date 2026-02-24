import ActionQueuePanel from '../../widgets/ActionQueuePanel/ActionQueuePanel.jsx';
import CreateSignalPanel from '../../widgets/CreateSignalPanel/CreateSignalPanel.jsx';
import ExecutiveBrief from '../../widgets/ExecutiveBrief/ExecutiveBrief.jsx';
import IntegrationsPanel from '../../widgets/IntegrationsPanel/IntegrationsPanel.jsx';
import KpiGrid from '../../widgets/KpiGrid/KpiGrid.jsx';
import SignalFeedPanel from '../../widgets/SignalFeedPanel/SignalFeedPanel.jsx';
import './CommandCenterScreen.css';

function CommandCenterScreen() {
  return (
    <section className="command-center-screen">
      <KpiGrid />
      <ExecutiveBrief />

      <section className="dashboard-grid command-center-grid">
        <aside className="control-rail">
          <IntegrationsPanel />
          <CreateSignalPanel />
        </aside>

        <section className="workspace">
          <ActionQueuePanel />
          <SignalFeedPanel />
        </section>
      </section>
    </section>
  );
}

export default CommandCenterScreen;
