export type ModelTemplate = {
  id: string;
  label: string;
  envKey: string;
};

export type ProviderTemplate = {
  id: string;
  name: string;
  keyUrl: string;
  baseUrl: string;
  models: ModelTemplate[];
};

const models = (envKey: string, entries: Array<[string, string]>): ModelTemplate[] =>
  entries.map(([id, label]) => ({ id, label, envKey }));

export const providerTemplates: ProviderTemplate[] = [
  { id: 'openai', name: 'OpenAI', keyUrl: 'https://platform.openai.com/api-keys', baseUrl: 'https://api.openai.com/v1', models: models('OPENAI_MODEL', [['gpt-5.5', 'GPT-5.5'], ['gpt-4.1', 'GPT-4.1'], ['gpt-4o', 'GPT-4o']]) },
  { id: 'anthropic', name: 'Anthropic', keyUrl: 'https://console.anthropic.com/settings/keys', baseUrl: 'https://api.anthropic.com', models: models('ANTHROPIC_MODEL', [['claude-opus-4-1', 'Claude Opus'], ['claude-sonnet-4-0', 'Claude Sonnet'], ['claude-3-7-sonnet-latest', 'Claude 3.7 Sonnet']]) },
  { id: 'gemini', name: 'Google Gemini', keyUrl: 'https://aistudio.google.com/apikey', baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai', models: models('GEMINI_MODEL', [['gemini-2.5-pro', 'Gemini 2.5 Pro'], ['gemini-2.5-flash', 'Gemini 2.5 Flash'], ['gemini-2.0-flash', 'Gemini 2.0 Flash']]) },
  { id: 'deepseek', name: 'DeepSeek', keyUrl: 'https://platform.deepseek.com/api_keys', baseUrl: 'https://api.deepseek.com', models: models('DEEPSEEK_MODEL', [['deepseek-chat', 'DeepSeek Chat'], ['deepseek-reasoner', 'DeepSeek Reasoner']]) },
  { id: 'qwen', name: '通义千问 / Qwen', keyUrl: 'https://bailian.console.aliyun.com/?apiKey=1', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', models: models('QWEN_MODEL', [['qwen-plus', 'Qwen Plus'], ['qwen-turbo', 'Qwen Turbo'], ['qwen-max', 'Qwen Max']]) },
  { id: 'glm', name: '智谱 GLM', keyUrl: 'https://bigmodel.cn/usercenter/apikeys', baseUrl: 'https://open.bigmodel.cn/api/paas/v4', models: models('GLM_MODEL', [['glm-4.5', 'GLM-4.5'], ['glm-4-flash', 'GLM-4 Flash']]) },
  { id: 'kimi', name: 'Kimi / 月之暗面', keyUrl: 'https://platform.moonshot.cn/console/api-keys', baseUrl: 'https://api.moonshot.cn/v1', models: models('MOONSHOT_MODEL', [['kimi-k2', 'Kimi K2'], ['moonshot-v1-128k', 'Moonshot V1 128K']]) },
  { id: 'minimax', name: 'MiniMax', keyUrl: 'https://platform.minimaxi.com/user-center/basic-information/interface-key', baseUrl: 'https://api.minimaxi.com/v1', models: models('MINIMAX_MODEL', [['MiniMax-Text-01', 'MiniMax Text 01'], ['MiniMax-M1', 'MiniMax M1']]) },
  { id: 'xai', name: 'xAI / Grok', keyUrl: 'https://console.x.ai/', baseUrl: 'https://api.x.ai/v1', models: models('XAI_MODEL', [['grok-3', 'Grok 3'], ['grok-3-mini', 'Grok 3 Mini']]) },
  { id: 'openrouter', name: 'OpenRouter', keyUrl: 'https://openrouter.ai/keys', baseUrl: 'https://openrouter.ai/api/v1', models: models('OPENROUTER_MODEL', [['openai/gpt-4o', 'OpenAI GPT-4o'], ['deepseek/deepseek-chat', 'DeepSeek Chat'], ['anthropic/claude-3.7-sonnet', 'Claude 3.7 Sonnet']]) },
  { id: 'volcengine', name: '火山引擎 / 豆包', keyUrl: 'https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey', baseUrl: 'https://ark.cn-beijing.volces.com/api/v3', models: models('ARK_MODEL', [['doubao-1-5-pro-32k-250115', 'Doubao Pro'], ['doubao-1-5-lite-32k-250115', 'Doubao Lite']]) },
  { id: 'hunyuan', name: '腾讯混元', keyUrl: 'https://console.cloud.tencent.com/hunyuan/start', baseUrl: 'https://api.hunyuan.cloud.tencent.com/v1', models: models('HUNYUAN_MODEL', [['hunyuan-turbos-latest', 'Hunyuan Turbo'], ['hunyuan-pro', 'Hunyuan Pro']]) },
  { id: 'stepfun', name: '阶跃星辰 StepFun', keyUrl: 'https://platform.stepfun.com/interface-key', baseUrl: 'https://api.stepfun.com/v1', models: models('STEPFUN_MODEL', [['step-2-16k', 'Step-2 16K'], ['step-1v-8k', 'Step-1V 8K']]) },
  { id: 'mistral', name: 'Mistral AI', keyUrl: 'https://console.mistral.ai/api-keys/', baseUrl: 'https://api.mistral.ai/v1', models: models('MISTRAL_MODEL', [['mistral-large-latest', 'Mistral Large'], ['codestral-latest', 'Codestral']]) },
  { id: 'groq', name: 'Groq', keyUrl: 'https://console.groq.com/keys', baseUrl: 'https://api.groq.com/openai/v1', models: models('GROQ_MODEL', [['llama-3.3-70b-versatile', 'Llama 3.3 70B'], ['qwen-qwq-32b', 'Qwen QwQ 32B']]) },
  { id: 'custom', name: '自定义大模型', keyUrl: '', baseUrl: '', models: [{ id: 'custom-model', label: '自定义模型 ID', envKey: 'MODEL_NAME' }] },
];

export function providerTemplate(id: string): ProviderTemplate {
  return providerTemplates.find((provider) => provider.id === id) ?? providerTemplates[providerTemplates.length - 1];
}

const apiKeyEnvKeys: Record<string, string> = {
  openai: 'OPENAI_API_KEY', anthropic: 'ANTHROPIC_API_KEY', gemini: 'GEMINI_API_KEY', deepseek: 'DEEPSEEK_API_KEY',
  qwen: 'DASHSCOPE_API_KEY', glm: 'ZHIPUAI_API_KEY', kimi: 'MOONSHOT_API_KEY', minimax: 'MINIMAX_API_KEY',
  xai: 'XAI_API_KEY', openrouter: 'OPENROUTER_API_KEY', volcengine: 'ARK_API_KEY', hunyuan: 'HUNYUAN_API_KEY',
  stepfun: 'STEPFUN_API_KEY', mistral: 'MISTRAL_API_KEY', groq: 'GROQ_API_KEY',
};

export function defaultApiKeyEnvKey(providerId: string): string {
  return apiKeyEnvKeys[providerId] ?? '';
}

export function defaultEndpointEnvKey(providerId: string): string {
  return providerId === 'qwen' ? 'DASHSCOPE_BASE_URL' : providerId === 'glm' ? 'ZHIPUAI_BASE_URL' : providerId === 'kimi' ? 'MOONSHOT_BASE_URL' : providerId === 'volcengine' ? 'ARK_BASE_URL' : providerId === 'hunyuan' ? 'HUNYUAN_BASE_URL' : providerId === 'stepfun' ? 'STEPFUN_BASE_URL' : `${providerId.toUpperCase()}_BASE_URL`;
}
