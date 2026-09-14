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

const catalog = (envKey: string, ids: string[]): ModelTemplate[] =>
  models(envKey, ids.map((id) => [id, id]));

// Baseline mirrors EchoBird's modelDirectory.json. Vendor docs were checked for
// the current major providers; dynamic gateways still retain the custom option.
export const providerTemplates: ProviderTemplate[] = [
  { id: 'xiaomi', name: 'Xiaomi 小米', keyUrl: 'https://platform.xiaomimimo.com/', baseUrl: 'https://token-plan-cn.xiaomimimo.com/v1', models: catalog('MIMO_MODEL', ['mimo-v2.5-pro']) },
  { id: 'deepseek', name: 'DeepSeek', keyUrl: 'https://platform.deepseek.com/api_keys', baseUrl: 'https://api.deepseek.com', models: models('DEEPSEEK_MODEL', [['deepseek-v4-flash', 'DeepSeek V4 Flash'], ['deepseek-v4-pro', 'DeepSeek V4 Pro'], ['deepseek-v4-flash-vision-exp', 'DeepSeek V4 Flash Vision']]) },
  { id: 'minimax-global', name: 'MiniMax EN', keyUrl: 'https://platform.minimax.io/', baseUrl: 'https://api.minimax.io/v1', models: catalog('MINIMAX_MODEL', ['MiniMax-M2.7', 'MiniMax-M2.7-highspeed', 'MiniMax-M2.5', 'MiniMax-M2.5-highspeed', 'MiniMax-M2.1', 'MiniMax-M2.1-highspeed', 'MiniMax-M2']) },
  { id: 'minimax', name: 'MiniMax CN', keyUrl: 'https://platform.minimaxi.com/user-center/basic-information/interface-key', baseUrl: 'https://api.minimaxi.com/v1', models: catalog('MINIMAX_MODEL', ['MiniMax-M2.7', 'MiniMax-M2.7-highspeed', 'MiniMax-M2.5', 'MiniMax-M2.5-highspeed', 'MiniMax-M2.1', 'MiniMax-M2.1-highspeed', 'MiniMax-M2']) },
  { id: 'glm', name: 'GLM 智谱', keyUrl: 'https://bigmodel.cn/usercenter/apikeys', baseUrl: 'https://open.bigmodel.cn/api/coding/paas/v4', models: catalog('GLM_MODEL', ['glm-5.3', 'glm-5.1', 'glm-5']) },
  { id: 'zai', name: 'Z.ai', keyUrl: 'https://z.ai/manage-apikey/apikey-list', baseUrl: 'https://api.z.ai/api/coding/paas/v4', models: catalog('ZAI_MODEL', ['glm-5.3', 'glm-5.1', 'glm-5']) },
  { id: 'kimi', name: 'Kimi 月之暗面', keyUrl: 'https://platform.moonshot.cn/console/api-keys', baseUrl: 'https://api.moonshot.cn/v1', models: catalog('MOONSHOT_MODEL', ['kimi-k3', 'kimi-k2.7-code', 'kimi-k2.7-code-highspeed', 'kimi-k2.6']) },
  { id: 'kimi-global', name: 'Kimi Global', keyUrl: 'https://platform.moonshot.ai/console/api-keys', baseUrl: 'https://api.moonshot.ai/v1', models: catalog('MOONSHOT_MODEL', ['kimi-k3', 'kimi-k2.7-code', 'kimi-k2.7-code-highspeed', 'kimi-k2.6']) },
  { id: 'longcat', name: 'LongCat 美团', keyUrl: 'https://longcat.chat/platform/', baseUrl: 'https://api.longcat.chat/openai', models: catalog('LONGCAT_MODEL', ['LongCat-2.0']) },
  { id: 'openai', name: 'OpenAI', keyUrl: 'https://platform.openai.com/api-keys', baseUrl: 'https://api.openai.com/v1', models: catalog('OPENAI_MODEL', ['gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna', 'gpt-5.5', 'gpt-4.1']) },
  { id: 'gemini', name: 'Google Gemini', keyUrl: 'https://aistudio.google.com/apikey', baseUrl: 'https://generativelanguage.googleapis.com/v1beta/openai', models: catalog('GEMINI_MODEL', ['gemini-3.8-flash', 'gemini-3.7-flash', 'gemini-3.6-flash', 'gemini-3.5-flash', 'gemini-3.5-flash-lite', 'gemini-3.1-flash-lite', 'gemini-3.1-pro-preview', 'gemini-3-flash-preview']) },
  { id: 'byteplus', name: 'BytePlus', keyUrl: 'https://console.byteplus.com/ark/region:ark+ap-southeast-1/apiKey', baseUrl: 'https://ark.ap-southeast.bytepluses.com/api/coding/v3', models: catalog('BYTEPLUS_MODEL', ['ark-code-latest', 'doubao-seed-2.0-code', 'doubao-seed-2.0-pro', 'doubao-seed-2.0-lite', 'doubao-seed-2.0-mini', 'glm-5.3', 'kimi-k2.7-code', 'deepseek-v4-pro', 'deepseek-v4-flash', 'deepseek-v4-flash-vision-exp', 'minimax-m3', 'minimax-m2.7', 'kimi-k2.6']) },
  { id: 'ernie', name: 'ERNIE 百度千帆', keyUrl: 'https://console.bce.baidu.com/qianfan/ais/console/apiKey', baseUrl: 'https://qianfan.baidubce.com/v2/coding', models: catalog('ERNIE_MODEL', ['deepseek-v4-pro', 'deepseek-v4-flash', 'deepseek-v4-flash-vision-exp', 'glm-5.3', 'glm-5.1', 'kimi-k2.6', 'ernie-5.1']) },
  { id: 'opencode-zen', name: 'OpenCode Zen', keyUrl: 'https://opencode.ai/zen', baseUrl: 'https://opencode.ai/zen/v1', models: catalog('OPENCODE_MODEL', ['claude-opus-5', 'claude-sonnet-5', 'claude-haiku-4-5', 'gpt-5.5', 'gemini-3.7-flash', 'deepseek-v4-pro', 'deepseek-v4-flash', 'deepseek-v4-flash-vision-exp', 'glm-5.3', 'kimi-k3', 'qwen3.8-max', 'minimax-m3', 'grok-4.5']) },
  { id: 'opencode-go', name: 'OpenCode Go', keyUrl: 'https://opencode.ai/go', baseUrl: 'https://opencode.ai/zen/go/v1', models: catalog('OPENCODE_GO_MODEL', ['deepseek-v4-pro', 'deepseek-v4-flash', 'deepseek-v4-flash-vision-exp', 'glm-5.3', 'gpt-5.6-luna', 'grok-4.5', 'hy3', 'kimi-k3', 'mimo-v2.5-pro', 'minimax-m3', 'qwen3.8-max']) },
  { id: 'qwen', name: '通义千问 / Qwen', keyUrl: 'https://bailian.console.aliyun.com/?apiKey=1', baseUrl: 'https://dashscope.aliyuncs.com/compatible-mode/v1', models: catalog('QWEN_MODEL', ['qwen3.8-max', 'qwen3.7-max', 'qwen3.7-plus', 'qwen3.6-flash', 'qwen3.5-plus']) },
  { id: 'qianwen-platform', name: '千问AI平台', keyUrl: 'https://platform.qianwenai.com/', baseUrl: 'https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1', models: catalog('QWEN_MODEL', ['qwen3.8-max', 'qwen3.8-max-preview', 'qwen3.7-max', 'qwen3.7-plus', 'qwen3.6-flash', 'qwen-audio-3.0-tts-plus', 'glm-5.3', 'deepseek-v4-pro', 'deepseek-v4-flash-0731', 'deepseek-v4-flash-vision-exp']) },
  { id: 'anthropic', name: 'Anthropic', keyUrl: 'https://console.anthropic.com/settings/keys', baseUrl: 'https://api.anthropic.com', models: catalog('ANTHROPIC_MODEL', ['claude-fable-5-1', 'claude-opus-5', 'claude-sonnet-5', 'claude-haiku-4-5-20251001']) },
  { id: 'xai', name: 'xAI Grok', keyUrl: 'https://console.x.ai/', baseUrl: 'https://api.x.ai/v1', models: catalog('XAI_MODEL', ['grok-4.6', 'grok-4.5', 'grok-4.3', 'grok-4']) },
  { id: 'hunyuan', name: 'Hunyuan 腾讯混元 TokenHub', keyUrl: 'https://console.cloud.tencent.com/tokenhub/codingplan', baseUrl: 'https://api.lkeap.cloud.tencent.com/plan/v3', models: catalog('HUNYUAN_MODEL', ['hy4-preview', 'hy3', 'hy-mt2-pro', 'hy-mt2-plus', 'deepseek-v4-flash', 'deepseek-v4-pro', 'minimax-m2.7', 'glm-5.1', 'kimi-k2.6']) },
  { id: 'meta', name: 'Meta AI', keyUrl: 'https://ai.meta.com/', baseUrl: 'https://api.llama.com/v1', models: catalog('META_MODEL', ['Llama-4-Maverick-17B-128E-Instruct', 'Llama-4-Scout-17B-16E-Instruct']) },
  { id: 'perplexity', name: 'Perplexity', keyUrl: 'https://www.perplexity.ai/settings/api', baseUrl: 'https://api.perplexity.ai', models: catalog('PERPLEXITY_MODEL', ['sonar-pro', 'sonar', 'sonar-reasoning-pro', 'sonar-reasoning']) },
  { id: 'stepfun', name: 'Stepfun 阶跃星辰', keyUrl: 'https://platform.stepfun.com/interface-key', baseUrl: 'https://api.stepfun.com/v1', models: catalog('STEPFUN_MODEL', ['step-3.5-flash-2603', 'step-3.5-flash']) },
  { id: 'mistral', name: 'Mistral AI', keyUrl: 'https://console.mistral.ai/api-keys', baseUrl: 'https://api.mistral.ai/v1', models: catalog('MISTRAL_MODEL', ['mistral-medium-latest', 'mistral-small-latest', 'mistral-large-latest', 'codestral-latest', 'devstral-small-latest']) },
  { id: 'cohere', name: 'Cohere', keyUrl: 'https://dashboard.cohere.com/api-keys', baseUrl: 'https://api.cohere.ai/compatibility/v1', models: catalog('COHERE_MODEL', ['command-a-03-2025', 'command-r-plus', 'command-r', 'c4ai-aya-expanse-32b']) },
  { id: 'groq', name: 'Groq', keyUrl: 'https://console.groq.com/keys', baseUrl: 'https://api.groq.com/openai/v1', models: catalog('GROQ_MODEL', ['openai/gpt-oss-120b', 'openai/gpt-oss-20b', 'qwen/qwen3.8-27b', 'qwen/qwen3.6-27b', 'minimaxai/minimax-m2.7', 'groq/compound', 'groq/compound-mini']) },
  { id: 'together', name: 'Together AI', keyUrl: 'https://api.together.ai/settings/api-keys', baseUrl: 'https://api.together.xyz/v1', models: catalog('TOGETHER_MODEL', ['Qwen/Qwen3.8-Next-80B-A3B-Instruct', 'meta-llama/Llama-4-Maverick-17B-128E-Instruct-FP8', 'deepseek-ai/DeepSeek-V3', 'meta-llama/Llama-3.3-70B-Instruct-Turbo']) },
  { id: 'nvidia', name: 'NVIDIA NIM', keyUrl: 'https://build.nvidia.com/models', baseUrl: 'https://integrate.api.nvidia.com/v1', models: catalog('NVIDIA_MODEL', ['nvidia/llama-3.3-nemotron-super-49b-v1', 'openai/gpt-oss-120b', 'qwen/qwen3.5-397b-a17b']) },
  { id: 'agnes', name: 'Agnes AI', keyUrl: 'https://platform.agnes-ai.com/', baseUrl: 'https://apihub.agnes-ai.com/v1', models: catalog('AGNES_MODEL', ['agnes-2.0-flash']) },
  { id: 'volcengine', name: '火山引擎 / 豆包', keyUrl: 'https://console.volcengine.com/ark/region:ark+cn-beijing/apiKey', baseUrl: 'https://ark.cn-beijing.volces.com/api/coding/v3', models: catalog('ARK_MODEL', ['ark-code-latest', 'doubao-seed-2.0-code', 'doubao-seed-2.0-pro', 'doubao-seed-2.0-lite', 'doubao-seed-2.0-mini', 'glm-5.3', 'kimi-k2.7-code', 'deepseek-v4-pro', 'deepseek-v4-flash', 'deepseek-v4-flash-vision-exp', 'minimax-m3', 'minimax-m2.7', 'kimi-k2.6']) },
  { id: 'openrouter', name: 'OpenRouter', keyUrl: 'https://openrouter.ai/settings/keys', baseUrl: 'https://openrouter.ai/api/v1', models: catalog('OPENROUTER_MODEL', ['openai/gpt-5.6-sol', 'anthropic/claude-opus-5', 'google/gemini-3.8-flash', 'deepseek/deepseek-v4-pro', 'x-ai/grok-4.6', 'qwen/qwen3.8-max']) },
  { id: 'cc-vibe', name: 'CC Vibe', keyUrl: 'https://cc-vibe.com/', baseUrl: 'https://cc-vibe.com/v1', models: catalog('CC_VIBE_MODEL', ['claude-opus-5', 'claude-sonnet-5', 'claude-fable-5', 'claude-fable-5-1', 'claude-haiku-4-5', 'gpt-5.5', 'gpt-5.6-luna', 'gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-6-astra', 'glm-5.3', 'kimi-k2.7']) },
  { id: 'apimart', name: 'APIMart', keyUrl: 'https://go.apimart.ai/gh-echobird', baseUrl: 'https://api.apimart.ai/v1', models: catalog('APIMART_MODEL', ['claude-opus-5', 'claude-fable-5-1', 'grok-4.6', 'qwen3.8-max', 'gemini-3.7-flash', 'kimi-k3', 'minimax-m3', 'glm-5.3', 'mimo-v2.5-pro', 'gpt-5.6-sol', 'gpt-6-astra', 'deepseek-v4-pro', 'deepseek-v4-flash', 'deepseek-v4-flash-vision-exp']) },
  { id: '88api', name: '88API', keyUrl: 'https://88api.ai/sign-up?aff=knFS', baseUrl: 'https://88api.ai/v1', models: catalog('API88_MODEL', ['gpt-5.6-sol', 'gpt-6-astra', 'deepseek-v4-flash', 'claude-opus-5', 'claude-sonnet-5', 'claude-fable-5', 'claude-fable-5-1', 'grok-4.6', 'gemini-3.8-flash', 'glm-5.3', 'kimi-k3']) },
  { id: 'grooroute', name: 'GrooRoute', keyUrl: 'https://grooroute.com/register?aff=FWGVPMYENJQ8', baseUrl: 'https://grooroute.com/v1', models: catalog('GROOROUTE_MODEL', ['gpt-5.6-sol', 'gpt-6-astra', 'gpt-5.6-terra', 'gpt-5.6-luna', 'gpt-5.5', 'claude-opus-5', 'claude-sonnet-5', 'claude-fable-5', 'claude-fable-5-1', 'claude-haiku-4-5']) },
  { id: 'custom', name: '自定义大模型', keyUrl: '', baseUrl: '', models: [{ id: 'custom-model', label: '自定义模型 ID', envKey: 'MODEL_NAME' }] },
];

const apiKeyEnvKeys: Record<string, string> = {
  openai: 'OPENAI_API_KEY', anthropic: 'ANTHROPIC_API_KEY', gemini: 'GEMINI_API_KEY', deepseek: 'DEEPSEEK_API_KEY',
  xiaomi: 'MIMO_API_KEY', 'minimax-global': 'MINIMAX_API_KEY', minimax: 'MINIMAX_API_KEY', glm: 'ZHIPUAI_API_KEY', zai: 'ZAI_API_KEY',
  kimi: 'MOONSHOT_API_KEY', 'kimi-global': 'MOONSHOT_API_KEY', longcat: 'LONGCAT_API_KEY', qwen: 'DASHSCOPE_API_KEY', 'qianwen-platform': 'QWEN_API_KEY',
  byteplus: 'BYTEPLUS_API_KEY', ernie: 'QIANFAN_API_KEY', 'opencode-zen': 'OPENCODE_API_KEY', 'opencode-go': 'OPENCODE_API_KEY',
  xai: 'XAI_API_KEY', hunyuan: 'HUNYUAN_API_KEY', meta: 'META_API_KEY', perplexity: 'PERPLEXITY_API_KEY', stepfun: 'STEPFUN_API_KEY',
  mistral: 'MISTRAL_API_KEY', cohere: 'COHERE_API_KEY', groq: 'GROQ_API_KEY', together: 'TOGETHER_API_KEY', nvidia: 'NVIDIA_API_KEY',
  agnes: 'AGNES_API_KEY', volcengine: 'ARK_API_KEY', openrouter: 'OPENROUTER_API_KEY', 'cc-vibe': 'CC_VIBE_API_KEY', apimart: 'APIMART_API_KEY',
  '88api': 'API88_API_KEY', grooroute: 'GROOROUTE_API_KEY',
};

export function providerTemplate(id: string): ProviderTemplate {
  return providerTemplates.find((provider) => provider.id === id) ?? providerTemplates[providerTemplates.length - 1];
}

export function defaultApiKeyEnvKey(providerId: string): string {
  return apiKeyEnvKeys[providerId] ?? '';
}

export function defaultEndpointEnvKey(providerId: string): string {
  const aliases: Record<string, string> = { qwen: 'DASHSCOPE_BASE_URL', 'qianwen-platform': 'QWEN_BASE_URL', glm: 'ZHIPUAI_BASE_URL', kimi: 'MOONSHOT_BASE_URL', 'kimi-global': 'MOONSHOT_BASE_URL', volcengine: 'ARK_BASE_URL', hunyuan: 'HUNYUAN_BASE_URL' };
  return aliases[providerId] ?? `${providerId.toUpperCase().replaceAll('-', '_')}_BASE_URL`;
}
