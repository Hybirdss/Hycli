import type { Provider } from './types';

// Saved configuration is usable for inference; it is not live account evidence.
export function providerStatus(provider: Provider): string {
  if (provider.id !== 'codex') return provider.connected ? 'connections.connected' : 'connections.notConnected';
  if (!provider.connected) return 'connections.notConnected';
  if (!provider.available) return 'actions.unverified';
  if (provider.verification === 'verified') return 'connections.connected';
  if (provider.verification === 'checking') return 'connections.testing';
  if (provider.verification === 'signed_out') return 'connections.notConnected';
  return 'actions.unverified';
}
