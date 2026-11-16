import type { CapabilityModelRow } from './lib/backend';

export type SecretMetadata = {
  id: string;
  name: string;
  envKey: string;
  provider: string;
  description?: string;
  tags: string[];
  status: string;
  /** Legacy free-form label, only used when updatedAt is missing (demo data). */
  updated: string;
  /** Unix seconds of the last change. */
  updatedAt?: number;
  /** Unix seconds of the last key validation; 0 or missing means never. */
  validatedAt?: number;
  favorite?: boolean;
  valueType?: string;
  category?: string;
  scope?: string;
  archived?: boolean;
  modelId?: string;
  modelEnvKey?: string;
  endpointUrl?: string;
  endpointEnvKey?: string;
  modelsSyncedAt?: number;
  modelsSyncStatus?: string;
  /** Plaintext list line computed by the backend (host, bank and last digits, env name). */
  subtitle?: string;
  /** Logins only: exposed to AI and exports as PREFIX_USERNAME / PREFIX_PASSWORD. */
  devUse?: boolean;
  /** Config groups only: AI may read the values of plain variables. */
  aiValues?: boolean;
};

export type CredentialPreset = { provider: string; name: string; envKey: string; modelId: string; modelEnvKey: string; endpointUrl: string; endpointEnvKey: string };

export type SecretRequest = { name: string; provider_id: string; env_key: string; description: string; tags: string[]; value: string; value_type: string; category: string; scope: string; favorite: boolean; archived: boolean; model_id: string; model_env_key: string; endpoint_url: string; endpoint_env_key: string; capability_models?: CapabilityModelRow[] };

export type CredentialType = 'api_key';
