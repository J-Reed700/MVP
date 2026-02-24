import { useAppContext } from '../../../context/AppContext.jsx';
import './SettingsScreen.css';

function SettingsScreen() {
  const {
    loadingLlmSettings,
    handleSaveLlmSettings,
    llmSettings,
    updateLlmField,
    handleProviderChange,
    llmModelOptions,
    isOpenAiProvider,
    savingLlmSettings,
    handleLoadLlmModels,
    loadingLlmModels,
    handleWarmupLlm,
    warmingLlm,
    llmStatus,
  } = useAppContext();

  return (
    <section className="settings-screen settings-screen-block">
      <article className="panel">
        <h2>LLM Settings</h2>
        {loadingLlmSettings ? (
          <p className="meta">Loading settings...</p>
        ) : (
          <form className="signal-form settings-form" onSubmit={handleSaveLlmSettings}>
            <section className="settings-section">
              <h3>General</h3>
              <div className="settings-grid">
                <label>
                  <span>Enable LLM Analytics</span>
                  <select
                    value={llmSettings.enabled ? 'enabled' : 'disabled'}
                    onChange={(event) => updateLlmField('enabled', event.target.value === 'enabled')}
                  >
                    <option value="enabled">Enabled</option>
                    <option value="disabled">Disabled</option>
                  </select>
                </label>

                <label>
                  Provider
                  <select
                    value={llmSettings.provider}
                    onChange={(event) => handleProviderChange(event.target.value)}
                  >
                    <option value="openai">OpenAI-compatible</option>
                    <option value="ollama">Ollama</option>
                  </select>
                </label>

                <label>
                  Base URL
                  <input
                    value={llmSettings.base_url}
                    onChange={(event) => updateLlmField('base_url', event.target.value)}
                    placeholder="http://localhost:11434"
                  />
                </label>

                <label>
                  Model
                  <select
                    value={llmSettings.model}
                    onChange={(event) => updateLlmField('model', event.target.value)}
                  >
                    {llmModelOptions.length === 0 ? (
                      <option value={llmSettings.model || ''}>{llmSettings.model || 'No models loaded'}</option>
                    ) : (
                      llmModelOptions.map((model) => (
                        <option key={model} value={model}>
                          {model}
                        </option>
                      ))
                    )}
                  </select>
                </label>

                <label>
                  Timeout Seconds
                  <input
                    type="number"
                    min="60"
                    max="300"
                    value={llmSettings.timeout_seconds}
                    onChange={(event) => updateLlmField('timeout_seconds', event.target.value)}
                  />
                </label>
              </div>
            </section>

            {isOpenAiProvider && (
              <section className="settings-section">
                <h3>Provider Credentials</h3>
                <div className="settings-grid">
                  <label>
                    API Key
                    <input
                      type="password"
                      value={llmSettings.api_key}
                      onChange={(event) => updateLlmField('api_key', event.target.value)}
                      placeholder="sk-..."
                    />
                  </label>
                </div>
              </section>
            )}

            <section className="settings-section">
              <h3>Endpoint Authentication</h3>
              <p className="meta settings-hint">
                For protected endpoints behind proxies like Pangolin, set header auth here. Basic auth is optional.
                These headers are attached to both `/v1/*` and `/api/*` calls.
              </p>
              <div className="settings-grid">
                <label>
                  Basic Auth User
                  <input
                    value={llmSettings.basic_auth_user}
                    onChange={(event) => updateLlmField('basic_auth_user', event.target.value)}
                    placeholder="svc_user"
                  />
                </label>

                <label>
                  Basic Auth Pass
                  <input
                    type="password"
                    value={llmSettings.basic_auth_pass}
                    onChange={(event) => updateLlmField('basic_auth_pass', event.target.value)}
                    placeholder="svc_pass"
                  />
                </label>

                <label>
                  Header 1 Name
                  <input
                    value={llmSettings.user_header_name}
                    onChange={(event) => updateLlmField('user_header_name', event.target.value)}
                    placeholder="X-Auth-User"
                  />
                </label>

                <label>
                  Header 1 Value
                  <input
                    value={llmSettings.user_header_value}
                    onChange={(event) => updateLlmField('user_header_value', event.target.value)}
                    placeholder="svc_user"
                  />
                </label>

                <label>
                  Header 2 Name
                  <input
                    value={llmSettings.pass_header_name}
                    onChange={(event) => updateLlmField('pass_header_name', event.target.value)}
                    placeholder="X-Auth-Pass"
                  />
                </label>

                <label>
                  Header 2 Value
                  <input
                    type="password"
                    value={llmSettings.pass_header_value}
                    onChange={(event) => updateLlmField('pass_header_value', event.target.value)}
                    placeholder="svc_pass"
                  />
                </label>
              </div>
            </section>

            <button disabled={savingLlmSettings} type="submit">
              {savingLlmSettings ? 'Saving Settings...' : 'Save Settings'}
            </button>
          </form>
        )}

        <div className="inline-button-row">
          <button
            className="secondary"
            type="button"
            onClick={handleLoadLlmModels}
            disabled={loadingLlmModels}
          >
            {loadingLlmModels ? 'Loading Models...' : 'Load Models'}
          </button>
          <button className="secondary" type="button" onClick={handleWarmupLlm} disabled={warmingLlm}>
            {warmingLlm ? 'Warming Up...' : 'Warm Up Model'}
          </button>
        </div>
        {llmStatus && <p className="meta success">{llmStatus}</p>}
      </article>
    </section>
  );
}

export default SettingsScreen;
