import { createContext, useContext, useEffect, useMemo, useState } from 'react';
import {
  applySignalAction,
  createSignal,
  fetchInsights,
  fetchLlmModels,
  fetchLlmSettings,
  fetchSignals,
  resetStoryDataset,
  saveLlmSettings,
  syncJira,
  syncGong,
  warmupLlm,
} from '../api.js';

const AppContext = createContext(null);

const INITIAL_FORM = {
  title: '',
  summary: '',
  owner: '',
  sourceSystems: '',
  tags: '',
};

const INITIAL_LLM_SETTINGS = {
  enabled: false,
  provider: 'openai',
  model: 'gpt-4.1-mini',
  base_url: 'https://api.openai.com/v1',
  api_key: '',
  timeout_seconds: 120,
  basic_auth_user: '',
  basic_auth_pass: '',
  user_header_name: '',
  user_header_value: '',
  pass_header_name: '',
  pass_header_value: '',
};

const PRIORITY_ORDER = { high: 3, medium: 2, low: 1 };
const PAGE_SIZE = 20;

function splitCsv(value) {
  return value
    .split(',')
    .map((part) => part.trim())
    .filter(Boolean);
}

function parseDate(value) {
  const parsed = new Date(value ?? '');
  return Number.isNaN(parsed.getTime()) ? null : parsed;
}

function formatDate(value) {
  const parsed = parseDate(value);
  if (!parsed) {
    return 'Unknown';
  }
  return new Intl.DateTimeFormat(undefined, {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
  }).format(parsed);
}

function formatRelative(value) {
  const parsed = parseDate(value);
  if (!parsed) {
    return 'Unknown recency';
  }

  const diffMs = Date.now() - parsed.getTime();
  const day = 24 * 60 * 60 * 1000;
  const days = Math.floor(diffMs / day);

  if (days <= 0) {
    return 'Updated today';
  }
  if (days === 1) {
    return 'Updated yesterday';
  }
  if (days < 30) {
    return `Updated ${days}d ago`;
  }

  const months = Math.floor(days / 30);
  return `Updated ${months}mo ago`;
}

function normalizePriority(priority) {
  const key = (priority ?? 'medium').toLowerCase();
  return PRIORITY_ORDER[key] ? key : 'medium';
}

function insightScore(insight) {
  return PRIORITY_ORDER[normalizePriority(insight.priority)] ?? PRIORITY_ORDER.medium;
}

function insightFamily(category) {
  const value = (category ?? '').toLowerCase();
  if (['missing_owners', 'owner_concentration_risk', 'source_owner_gap'].includes(value)) {
    return 'ownership';
  }
  if (['renewal_window_risk', 'arr_exposure_risk', 'competitive_pressure_risk'].includes(value)) {
    return 'revenue_risk';
  }
  if (['account_signal_hotspots', 'nps_follow_up_queue', 'discovery_quality_gap'].includes(value)) {
    return 'customer_execution';
  }
  if (['severity_burden', 'industry_cluster_risk'].includes(value)) {
    return 'service_reliability';
  }
  if (['source_dependency_risk', 'evidence_confidence_gap', 'metadata_hygiene_gap'].includes(value)) {
    return 'evidence_quality';
  }
  if (['expansion_momentum'].includes(value)) {
    return 'growth';
  }
  return 'general';
}

function selectTopInsightsWithDiversity(insights, limit = 5) {
  if (!Array.isArray(insights) || insights.length <= limit) {
    return insights ?? [];
  }

  const selected = [];
  const usedFamilies = new Set();

  for (const insight of insights) {
    const family = insightFamily(insight.category);
    if (!usedFamilies.has(family)) {
      selected.push(insight);
      usedFamilies.add(family);
      if (selected.length >= limit) {
        return selected;
      }
    }
  }

  for (const insight of insights) {
    if (selected.includes(insight)) {
      continue;
    }
    selected.push(insight);
    if (selected.length >= limit) {
      break;
    }
  }

  return selected;
}

function isSyntheticSignal(signal) {
  const tags = signal.tags ?? [];
  return (
    tags.includes('synthetic') ||
    tags.includes('batch') ||
    signal.title?.toLowerCase().includes('synthetic')
  );
}

function supportsAutomation(category) {
  return [
    'missing_owners',
    'source_owner_gap',
    'stale_signals',
    'superseded_records',
    'possible_duplicate_signals',
    'renewal_window_risk',
    'arr_exposure_risk',
    'industry_cluster_risk',
    'severity_burden',
    'evidence_confidence_gap',
    'account_signal_hotspots',
    'nps_follow_up_queue',
    'competitive_pressure_risk',
    'discovery_quality_gap',
    'expansion_momentum',
  ].includes(category);
}

function normalizeOllamaBaseUrl(baseUrl) {
  if (!baseUrl) {
    return baseUrl;
  }
  try {
    const parsed = new URL(baseUrl);
    const host = parsed.hostname.toLowerCase();
    const isLocalhost = host === 'localhost' || host === '127.0.0.1' || host === '0.0.0.0';
    if (parsed.protocol === 'http:' && !isLocalhost) {
      parsed.protocol = 'https:';
      return parsed.toString().replace(/\/$/, '');
    }
    return baseUrl;
  } catch {
    return baseUrl;
  }
}

function toLlmSettingsPayload(settings) {
  const provider = settings.provider?.trim() || 'openai';
  const rawBaseUrl = settings.base_url?.trim() || '';
  const baseUrl = provider === 'ollama' ? normalizeOllamaBaseUrl(rawBaseUrl) : rawBaseUrl;
  const parsedTimeout = Number(settings.timeout_seconds);
  const fallbackTimeout = 120;
  const normalizedTimeout =
    Number.isFinite(parsedTimeout) && parsedTimeout > 0 ? parsedTimeout : fallbackTimeout;

  return {
    enabled: Boolean(settings.enabled),
    provider,
    model: settings.model?.trim() || '',
    base_url: baseUrl,
    api_key: provider === 'openai' ? settings.api_key?.trim() || undefined : undefined,
    timeout_seconds: Math.max(60, normalizedTimeout),
    basic_auth_user: settings.basic_auth_user?.trim() || undefined,
    basic_auth_pass: settings.basic_auth_pass?.trim() || undefined,
    user_header_name: settings.user_header_name?.trim() || undefined,
    user_header_value: settings.user_header_value?.trim() || undefined,
    pass_header_name: settings.pass_header_name?.trim() || undefined,
    pass_header_value: settings.pass_header_value?.trim() || undefined,
  };
}

function fromLlmSettingsResponse(payload) {
  const provider = payload?.provider ?? 'openai';
  const fallbackTimeout = 120;
  const parsedTimeout = Number(payload?.timeout_seconds ?? fallbackTimeout);
  const normalizedTimeout =
    Number.isFinite(parsedTimeout) && parsedTimeout > 0 ? parsedTimeout : fallbackTimeout;

  return {
    enabled: Boolean(payload?.enabled),
    provider,
    model: payload?.model ?? '',
    base_url: payload?.base_url ?? '',
    api_key: payload?.api_key ?? '',
    timeout_seconds: Math.max(60, normalizedTimeout),
    basic_auth_user: payload?.basic_auth_user ?? '',
    basic_auth_pass: payload?.basic_auth_pass ?? '',
    user_header_name: payload?.user_header_name ?? '',
    user_header_value: payload?.user_header_value ?? '',
    pass_header_name: payload?.pass_header_name ?? '',
    pass_header_value: payload?.pass_header_value ?? '',
  };
}

export function AppProvider({ children }) {
  const [signals, setSignals] = useState([]);
  const [insights, setInsights] = useState([]);
  const [form, setForm] = useState(INITIAL_FORM);
  const [error, setError] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [syncingJira, setSyncingJira] = useState(false);
  const [syncingGong, setSyncingGong] = useState(false);
  const [loadingStoryData, setLoadingStoryData] = useState(false);
  const [syncResult, setSyncResult] = useState('');
  const [jiraJql, setJiraJql] = useState('project is not EMPTY ORDER BY updated DESC');
  const [jiraLimit, setJiraLimit] = useState(250);
  const [jiraDefaultOwner, setJiraDefaultOwner] = useState('CS Operations');
  const [gongLimit, setGongLimit] = useState(500);
  const [gongDefaultOwner, setGongDefaultOwner] = useState('CS Operations');
  const [loading, setLoading] = useState(true);
  const [lastRefreshedAt, setLastRefreshedAt] = useState(null);
  const [llmSettings, setLlmSettings] = useState(INITIAL_LLM_SETTINGS);
  const [llmModels, setLlmModels] = useState([]);
  const [loadingLlmSettings, setLoadingLlmSettings] = useState(true);
  const [savingLlmSettings, setSavingLlmSettings] = useState(false);
  const [loadingLlmModels, setLoadingLlmModels] = useState(false);
  const [warmingLlm, setWarmingLlm] = useState(false);
  const [llmStatus, setLlmStatus] = useState('');
  const [activeScreen, setActiveScreen] = useState('command_center');

  const [audienceFilter, setAudienceFilter] = useState('all');
  const [query, setQuery] = useState('');
  const [sourceFilter, setSourceFilter] = useState('all');
  const [ownerFilter, setOwnerFilter] = useState('all');
  const [hideSynthetic, setHideSynthetic] = useState(true);
  const [page, setPage] = useState(1);
  const [runningAction, setRunningAction] = useState('');
  const [actionResult, setActionResult] = useState('');

  async function loadData() {
    setLoading(true);
    setError('');
    try {
      const [signalData, insightData] = await Promise.all([fetchSignals(), fetchInsights()]);
      setSignals(signalData);
      setInsights(insightData);
      setLastRefreshedAt(new Date());
    } catch (err) {
      setError(err.message);
    } finally {
      setLoading(false);
    }
  }

  async function loadLlmSettings() {
    setLoadingLlmSettings(true);
    try {
      const settings = await fetchLlmSettings();
      const parsed = fromLlmSettingsResponse(settings);
      setLlmSettings(parsed);
      setLlmModels((prev) => {
        const next = new Set(prev);
        if (parsed.model) {
          next.add(parsed.model);
        }
        return Array.from(next).sort((a, b) => a.localeCompare(b));
      });
    } catch (err) {
      setError(err.message);
    } finally {
      setLoadingLlmSettings(false);
    }
  }

  useEffect(() => {
    loadData();
    loadLlmSettings();
  }, []);

  useEffect(() => {
    setPage(1);
  }, [query, sourceFilter, ownerFilter, hideSynthetic]);

  const ownershipCoverage = useMemo(() => {
    if (!signals.length) {
      return 0;
    }
    const owned = signals.filter((signal) => Boolean(signal.owner)).length;
    return Math.round((owned / signals.length) * 100);
  }, [signals]);

  const staleCount = useMemo(() => {
    const threshold = Date.now() - 30 * 24 * 60 * 60 * 1000;
    return signals.filter((signal) => {
      const updated = parseDate(signal.updated_at);
      return updated ? updated.getTime() < threshold : false;
    }).length;
  }, [signals]);

  const sourceOptions = useMemo(() => {
    const values = new Set();
    for (const signal of signals) {
      for (const source of signal.source_systems ?? []) {
        values.add(source);
      }
    }
    return ['all', ...Array.from(values).sort((a, b) => a.localeCompare(b))];
  }, [signals]);

  const scopedInsights = useMemo(() => {
    const scoped = insights.filter((insight) => {
      if (audienceFilter === 'all') {
        return true;
      }
      return (insight.audience ?? 'manager').toLowerCase() === audienceFilter;
    });

    return scoped.sort((a, b) => {
      const score = insightScore(b) - insightScore(a);
      if (score !== 0) {
        return score;
      }
      const relatedA = (a.related_signal_ids ?? []).length;
      const relatedB = (b.related_signal_ids ?? []).length;
      return relatedB - relatedA;
    });
  }, [insights, audienceFilter]);

  const filteredSignals = useMemo(() => {
    const loweredQuery = query.trim().toLowerCase();

    return signals
      .filter((signal) => {
        if (hideSynthetic && isSyntheticSignal(signal)) {
          return false;
        }

        if (sourceFilter !== 'all' && !(signal.source_systems ?? []).includes(sourceFilter)) {
          return false;
        }

        if (ownerFilter === 'assigned' && !signal.owner) {
          return false;
        }

        if (ownerFilter === 'unassigned' && signal.owner) {
          return false;
        }

        if (!loweredQuery) {
          return true;
        }

        const haystack = [
          signal.title,
          signal.summary,
          signal.owner ?? '',
          ...(signal.tags ?? []),
          ...(signal.source_systems ?? []),
        ]
          .join(' ')
          .toLowerCase();

        return haystack.includes(loweredQuery);
      })
      .sort((a, b) => {
        const dateA = parseDate(a.updated_at)?.getTime() ?? 0;
        const dateB = parseDate(b.updated_at)?.getTime() ?? 0;
        return dateB - dateA;
      });
  }, [signals, query, sourceFilter, ownerFilter, hideSynthetic]);

  const totalPages = useMemo(() => Math.max(1, Math.ceil(filteredSignals.length / PAGE_SIZE)), [filteredSignals]);

  useEffect(() => {
    if (page > totalPages) {
      setPage(totalPages);
    }
  }, [page, totalPages]);

  const pagedSignals = useMemo(() => {
    const start = (page - 1) * PAGE_SIZE;
    return filteredSignals.slice(start, start + PAGE_SIZE);
  }, [filteredSignals, page]);

  const topActions = useMemo(() => selectTopInsightsWithDiversity(scopedInsights, 5), [scopedInsights]);
  const highPriorityCount = insights.filter((item) => normalizePriority(item.priority) === 'high').length;
  const dueSoonCount = insights.filter((item) => Number(item.due_in_days ?? 999) <= 3).length;
  const unassignedCount = signals.filter((signal) => !signal.owner).length;
  const syntheticCount = signals.filter((signal) => isSyntheticSignal(signal)).length;
  const topInsight = scopedInsights[0] ?? null;
  const signalFiltersActive =
    sourceFilter !== 'all' || ownerFilter !== 'all' || hideSynthetic || query.trim().length > 0;
  const signalFilterScopeLabel = [
    sourceFilter !== 'all' ? `Source: ${sourceFilter}` : null,
    ownerFilter !== 'all'
      ? `Ownership: ${ownerFilter === 'assigned' ? 'Assigned' : 'Unassigned'}`
      : null,
    hideSynthetic ? 'Focus Mode: On' : null,
    query.trim() ? `Search: "${query.trim()}"` : null,
  ]
    .filter(Boolean)
    .join(' · ');
  const llmModelOptions = useMemo(() => {
    const merged = new Set(llmModels);
    if (llmSettings.model) {
      merged.add(llmSettings.model);
    }
    return Array.from(merged).sort((a, b) => a.localeCompare(b));
  }, [llmModels, llmSettings.model]);
  const isOpenAiProvider = llmSettings.provider === 'openai';

  async function handleSubmit(event) {
    event.preventDefault();
    setSubmitting(true);
    setError('');

    try {
      await createSignal({
        title: form.title,
        summary: form.summary,
        owner: form.owner,
        source_systems: splitCsv(form.sourceSystems),
        tags: splitCsv(form.tags),
      });

      setForm(INITIAL_FORM);
      await loadData();
    } catch (err) {
      setError(err.message);
    } finally {
      setSubmitting(false);
    }
  }

  async function handleJiraSync(event) {
    event.preventDefault();
    setSyncingJira(true);
    setError('');
    setSyncResult('');

    try {
      const result = await syncJira({
        jql: jiraJql,
        limit: Math.max(1, Number(jiraLimit) || 250),
        default_owner: jiraDefaultOwner.trim() || undefined,
      });
      setSyncResult(
        `Jira sync complete: fetched ${result.fetched}, created ${result.created}, updated ${result.updated}, skipped ${result.skipped}.`
      );
      await loadData();
    } catch (err) {
      setError(err.message);
    } finally {
      setSyncingJira(false);
    }
  }

  async function handleLoadStoryDataset() {
    setLoadingStoryData(true);
    setError('');
    setSyncResult('');
    setActionResult('');
    try {
      const result = await resetStoryDataset();
      setSyncResult(`Story dataset loaded: ${result.loaded} curated records.`);
      await loadData();
    } catch (err) {
      setError(err.message);
    } finally {
      setLoadingStoryData(false);
    }
  }

  async function handleGongSync(event) {
    event.preventDefault();
    setSyncingGong(true);
    setError('');
    setSyncResult('');

    try {
      const result = await syncGong({
        limit: Math.max(1, Number(gongLimit) || 500),
        default_owner: gongDefaultOwner.trim() || undefined,
      });
      setSyncResult(
        `Gong sync complete: fetched ${result.fetched}, created ${result.created}, updated ${result.updated}, skipped ${result.skipped}.`
      );
      await loadData();
    } catch (err) {
      setError(err.message);
    } finally {
      setSyncingGong(false);
    }
  }

  async function handleRunInsightAction(insight) {
    const signalIds = insight.related_signal_ids ?? [];
    if (signalIds.length === 0) {
      setError('Selected insight has no related signals to update.');
      return;
    }

    let payload;
    if (insight.category === 'missing_owners' || insight.category === 'source_owner_gap') {
      payload = {
        action: 'assign_owner',
        signal_ids: signalIds,
        owner: jiraDefaultOwner.trim() || 'CS Operations',
        only_if_owner_missing: true,
      };
    } else if (insight.category === 'stale_signals') {
      payload = {
        action: 'set_status',
        signal_ids: signalIds,
        status: 'superseded',
      };
    } else if (insight.category === 'superseded_records') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'archived',
      };
    } else if (insight.category === 'possible_duplicate_signals') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'needs-dedup',
      };
    } else if (insight.category === 'renewal_window_risk') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'playbook:renewal-war-room',
      };
    } else if (insight.category === 'arr_exposure_risk') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'risk:arr-at-risk',
      };
    } else if (insight.category === 'industry_cluster_risk') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'playbook:industry-response',
      };
    } else if (insight.category === 'severity_burden') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'playbook:incident-command',
      };
    } else if (insight.category === 'evidence_confidence_gap') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'needs-evidence',
      };
    } else if (insight.category === 'account_signal_hotspots') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'follow-up:scheduled',
      };
    } else if (insight.category === 'nps_follow_up_queue') {
      payload = {
        action: 'add_tag',
        signal_ids: signalIds,
        tag: 'follow-up:nps',
      };
    } else {
      setError(
        'This insight does not have an auto-action yet. Add one in the playbook mapping to enable one-click execution.'
      );
      return;
    }

    setRunningAction(insight.category);
    setActionResult('');
    setError('');
    try {
      const result = await applySignalAction(payload);
      setActionResult(`${insight.title}: updated ${result.updated} signal(s).`);
      await loadData();
    } catch (err) {
      setError(err.message);
    } finally {
      setRunningAction('');
    }
  }

  function updateLlmField(field, value) {
    setLlmSettings((prev) => ({ ...prev, [field]: value }));
  }

  function handleProviderChange(provider) {
    setLlmSettings((prev) => {
      const next = { ...prev, provider };
      if (provider === 'ollama' && (!prev.base_url || prev.base_url.includes('api.openai.com'))) {
        next.base_url = 'http://localhost:11434';
      }
      if (provider === 'ollama' && Number(prev.timeout_seconds || 0) < 120) {
        next.timeout_seconds = 120;
      }
      if (provider === 'openai' && (!prev.base_url || prev.base_url.includes('localhost:11434'))) {
        next.base_url = 'https://api.openai.com/v1';
      }
      return next;
    });
  }

  async function handleSaveLlmSettings(event) {
    event.preventDefault();
    setSavingLlmSettings(true);
    setError('');
    setLlmStatus('');
    try {
      const saved = await saveLlmSettings(toLlmSettingsPayload(llmSettings));
      const parsed = fromLlmSettingsResponse(saved);
      setLlmSettings(parsed);
      setLlmStatus(
        parsed.enabled
          ? `Saved LLM settings (${parsed.provider} · ${parsed.model}).`
          : 'LLM analytics disabled.'
      );
      if (parsed.model) {
        setLlmModels((prev) => {
          const next = new Set(prev);
          next.add(parsed.model);
          return Array.from(next).sort((a, b) => a.localeCompare(b));
        });
      }
      await loadData();
    } catch (err) {
      setError(err.message);
    } finally {
      setSavingLlmSettings(false);
    }
  }

  async function handleLoadLlmModels() {
    setLoadingLlmModels(true);
    setError('');
    setLlmStatus('');
    try {
      const result = await fetchLlmModels({
        settings: toLlmSettingsPayload(llmSettings),
      });
      setLlmModels(result.models ?? []);
      if ((result.models ?? []).length > 0) {
        setLlmSettings((prev) => ({
          ...prev,
          provider: result.provider ?? prev.provider,
          model:
            result.models.includes(prev.model) && prev.model
              ? prev.model
              : (result.models[0] ?? prev.model),
        }));
      }
      setLlmStatus(
        `Loaded ${(result.models ?? []).length} model(s) from ${result.provider ?? llmSettings.provider}.`
      );
    } catch (err) {
      setError(err.message);
    } finally {
      setLoadingLlmModels(false);
    }
  }

  async function handleWarmupLlm() {
    setWarmingLlm(true);
    setError('');
    setLlmStatus('');
    try {
      const result = await warmupLlm({
        settings: toLlmSettingsPayload(llmSettings),
      });
      setLlmStatus(`Warm-up complete (${result.provider} · ${result.model}): ${result.message}`);
    } catch (err) {
      setError(err.message);
    } finally {
      setWarmingLlm(false);
    }
  }

  const value = {
    signals,
    insights,
    form,
    error,
    submitting,
    syncingJira,
    syncingGong,
    loadingStoryData,
    syncResult,
    jiraJql,
    jiraLimit,
    jiraDefaultOwner,
    gongLimit,
    gongDefaultOwner,
    loading,
    lastRefreshedAt,
    llmSettings,
    llmModels,
    loadingLlmSettings,
    savingLlmSettings,
    loadingLlmModels,
    warmingLlm,
    llmStatus,
    activeScreen,
    audienceFilter,
    query,
    sourceFilter,
    ownerFilter,
    hideSynthetic,
    page,
    runningAction,
    actionResult,
    ownershipCoverage,
    staleCount,
    sourceOptions,
    scopedInsights,
    filteredSignals,
    totalPages,
    pagedSignals,
    topActions,
    highPriorityCount,
    dueSoonCount,
    unassignedCount,
    syntheticCount,
    topInsight,
    signalFiltersActive,
    signalFilterScopeLabel,
    llmModelOptions,
    isOpenAiProvider,
    setForm,
    setJiraJql,
    setJiraLimit,
    setJiraDefaultOwner,
    setGongLimit,
    setGongDefaultOwner,
    setActiveScreen,
    setAudienceFilter,
    setQuery,
    setSourceFilter,
    setOwnerFilter,
    setHideSynthetic,
    setPage,
    loadData,
    handleSubmit,
    handleJiraSync,
    handleGongSync,
    handleLoadStoryDataset,
    handleRunInsightAction,
    updateLlmField,
    handleProviderChange,
    handleSaveLlmSettings,
    handleLoadLlmModels,
    handleWarmupLlm,
    formatDate,
    formatRelative,
    normalizePriority,
    supportsAutomation,
  };

  return <AppContext.Provider value={value}>{children}</AppContext.Provider>;
}

export function useAppContext() {
  const context = useContext(AppContext);
  if (!context) {
    throw new Error('useAppContext must be used within AppProvider');
  }
  return context;
}
