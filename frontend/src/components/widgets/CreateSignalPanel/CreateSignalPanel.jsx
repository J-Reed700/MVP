import { useAppContext } from '../../../context/AppContext.jsx';
import './CreateSignalPanel.css';

function CreateSignalPanel() {
  const { form, setForm, handleSubmit, submitting } = useAppContext();

  return (
    <article className="panel create-signal-panel">
      <h2>Create Signal</h2>
      <form className="signal-form" onSubmit={handleSubmit}>
        <label>
          Title
          <input
            required
            value={form.title}
            onChange={(event) => setForm((prev) => ({ ...prev, title: event.target.value }))}
            placeholder="Onboarding completion dropped for enterprise accounts"
          />
        </label>

        <label>
          Summary
          <textarea
            required
            rows={3}
            value={form.summary}
            onChange={(event) => setForm((prev) => ({ ...prev, summary: event.target.value }))}
            placeholder="Week-over-week activation declined after a rollout for high ARR segment."
          />
        </label>

        <label>
          Owner
          <input
            value={form.owner}
            onChange={(event) => setForm((prev) => ({ ...prev, owner: event.target.value }))}
            placeholder="CS Operations"
          />
        </label>

        <label>
          Source Systems
          <input
            required
            value={form.sourceSystems}
            onChange={(event) => setForm((prev) => ({ ...prev, sourceSystems: event.target.value }))}
            placeholder="Jira, Gong, Salesforce"
          />
        </label>

        <label>
          Tags
          <input
            value={form.tags}
            onChange={(event) => setForm((prev) => ({ ...prev, tags: event.target.value }))}
            placeholder="renewal-risk, adoption, enterprise"
          />
        </label>

        <button disabled={submitting} type="submit">
          {submitting ? 'Saving...' : 'Save Signal'}
        </button>
      </form>
    </article>
  );
}

export default CreateSignalPanel;
