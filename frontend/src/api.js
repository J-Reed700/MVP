const API_BASE_URL = import.meta.env.VITE_API_BASE_URL ?? 'http://localhost:8080';

async function request(path, options = {}) {
  const response = await fetch(`${API_BASE_URL}${path}`, {
    headers: {
      'Content-Type': 'application/json',
      ...(options.headers ?? {}),
    },
    ...options,
  });

  if (!response.ok) {
    let message = `API request failed (${response.status}) for ${path}`;
    try {
      const body = await response.json();
      if (typeof body?.message === 'string' && body.message.trim()) {
        message = body.message;
      }
    } catch {
      // Keep default message if body is not JSON.
    }

    throw new Error(message);
  }

  return response.json();
}

export function fetchSignals() {
  return request('/api/signals');
}

export function fetchInsights() {
  return request('/api/insights');
}

export function createSignal(payload) {
  return request('/api/signals', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function syncJira(payload) {
  return request('/api/integrations/jira/sync', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function syncGong(payload) {
  return request('/api/integrations/gong/sync', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function applySignalAction(payload) {
  return request('/api/signals/actions', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function resetStoryDataset() {
  return request('/api/dev/story/reset', {
    method: 'POST',
  });
}

export function fetchLlmSettings() {
  return request('/api/settings/llm');
}

export function saveLlmSettings(payload) {
  return request('/api/settings/llm', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function fetchLlmModels(payload) {
  return request('/api/settings/llm/models', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}

export function warmupLlm(payload) {
  return request('/api/settings/llm/warmup', {
    method: 'POST',
    body: JSON.stringify(payload),
  });
}
