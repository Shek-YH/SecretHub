export type Locale = 'zh-CN' | 'en-US';

export type Translation = {
  nav: { all: string; favorites: string; recent: string; providers: string; profiles: string; projects: string; tags: string; settings: string; secrets: string };
  header: { eyebrow: string; title: string; subtitle: string; search: string; add: string; language: string };
  list: { title: string; selected: string; empty: string; searchEmpty: string; provider: string; updated: string; valid: string; unknown: string; active: string };
  detail: { title: string; masked: string; provider: string; envKey: string; status: string; tags: string; notes: string; copy: string; reveal: string; validate: string; edit: string; delete: string; deleteConfirm: string; noSelection: string };
  actions: { export: string; import: string; profile: string; validate: string; archive: string; chooseFolder: string; preview: string; importFormat: string; importContent: string; importPreview: string; confirmImport: string; imported: string };
  form: { addTitle: string; editTitle: string; name: string; envKey: string; provider: string; value: string; notes: string; tags: string; cancel: string; save: string; required: string };
  security: { localOnly: string; encrypted: string; audit: string; locked: string; unlock: string; warning: string };
  auth: { setupTitle: string; unlockTitle: string; subtitle: string; password: string; confirmPassword: string; setup: string; unlock: string; minLength: string; mismatch: string };
  settings: { title: string; general: string; security: string; ai: string; theme: string; defaultExport: string; autoLock: string; requireAuth: string; language: string; backup: string; backupSaved: string; on: string; off: string };
  profile: { title: string; name: string; save: string; apply: string; saved: string; empty: string };
  project: { title: string; recent: string; record: string; empty: string };
  auditPanel: { title: string; operation: string; result: string; empty: string };
  mcp: { title: string; description: string; confirm: string; confirmed: string };
  footer: { version: string; vault: string; synced: string };
};

export const translations: Record<Locale, Translation> = {
  'zh-CN': {
    nav: { all: '全部', favorites: '收藏', recent: '最近使用', providers: '服务商', profiles: '配置组合', projects: '项目', tags: '标签', settings: '设置', secrets: '凭据' },
    header: { eyebrow: 'LOCAL-FIRST / AI-SAFE', title: '凭据中心', subtitle: '安全保存一次，按项目随处使用。', search: '搜索凭据、服务商或标签', add: '添加凭据', language: 'English' },
    list: { title: '凭据', selected: '已选择', empty: '还没有凭据', searchEmpty: '没有匹配的凭据', provider: '服务商', updated: '更新于', valid: '有效', unknown: '待验证', active: '使用中' },
    detail: { title: '凭据详情', masked: '已安全遮罩', provider: '服务商', envKey: '环境变量', status: '状态', tags: '标签', notes: '备注', copy: '复制', reveal: 'Reveal', validate: '验证', edit: '编辑', delete: '删除', deleteConfirm: '确定删除这条凭据？', noSelection: '选择一条凭据查看详情' },
    actions: { export: '导出', import: '导入', profile: '加入组合', validate: '批量验证', archive: '归档', chooseFolder: '选择项目目录', preview: '预览 .env', importFormat: '导入格式', importContent: '粘贴 .env 或 JSON 内容', importPreview: '预览字段', confirmImport: '确认并加密导入', imported: '已导入并加密保存' },
    form: { addTitle: '添加凭据', editTitle: '编辑凭据', name: '名称', envKey: '环境变量名', provider: '服务商', value: 'Secret 值', notes: '备注', tags: '标签（逗号分隔）', cancel: '取消', save: '保存凭据', required: '必填' },
    security: { localOnly: '仅保存在本机', encrypted: 'AES-256-GCM 加密', audit: '操作可审计且不记录明文', locked: '保险库已锁定', unlock: '解锁保险库', warning: '安全提示：Secret 值不会进入浏览器存储、日志或 AI 上下文。' },
    auth: { setupTitle: '创建本地保险库', unlockTitle: '解锁 SecretHub', subtitle: 'Master Password 只保存在本机，无法通过服务器找回。', password: 'Master Password', confirmPassword: '确认 Password', setup: '创建并解锁', unlock: '解锁保险库', minLength: '至少 12 个字符', mismatch: '两次输入的 Password 不一致' },
    settings: { title: '设置', general: '常规', security: '安全', ai: 'AI / MCP', theme: '主题', defaultExport: '默认导出文件', autoLock: '自动锁定', requireAuth: '导出前需要验证', language: '显示语言', backup: '导出加密备份', backupSaved: '加密备份已写入所选目录', on: '开启', off: '关闭' },
    profile: { title: '配置组合', name: '组合名称', save: '保存组合', apply: '应用组合', saved: '组合已保存', empty: '还没有配置组合' },
    project: { title: '项目历史', recent: '最近使用的项目', record: '记录当前项目', empty: '还没有项目记录' },
    auditPanel: { title: '操作审计', operation: '操作', result: '结果', empty: '还没有审计事件' },
    mcp: { title: 'AI 请求待确认', description: 'MCP 请求准备项目环境。请确认遮罩预览后再写入项目。', confirm: '确认导出', confirmed: '已确认并导出' },
    footer: { version: 'V1.0 MVP', vault: '本地保险库', synced: '已保护' },
  },
  'en-US': {
    nav: { all: 'All', favorites: 'Favorites', recent: 'Recent', providers: 'Providers', profiles: 'Profiles', projects: 'Projects', tags: 'Tags', settings: 'Settings', secrets: 'Secrets' },
    header: { eyebrow: 'LOCAL-FIRST / AI-SAFE', title: 'Secret catalog', subtitle: 'Store once. Use everywhere, project by project.', search: 'Search secrets, providers or tags', add: 'Add secret', language: '中文' },
    list: { title: 'Secrets', selected: 'selected', empty: 'No secrets yet', searchEmpty: 'No matching secrets', provider: 'Provider', updated: 'Updated', valid: 'Valid', unknown: 'Not validated', active: 'Active' },
    detail: { title: 'Secret details', masked: 'Safely masked', provider: 'Provider', envKey: 'Environment key', status: 'Status', tags: 'Tags', notes: 'Notes', copy: 'Copy', reveal: 'Reveal', validate: 'Validate', edit: 'Edit', delete: 'Delete', deleteConfirm: 'Delete this secret?', noSelection: 'Select a secret to view details' },
    actions: { export: 'Export', import: 'Import', profile: 'Add to profile', validate: 'Validate selected', archive: 'Archive', chooseFolder: 'Choose project folder', preview: 'Preview .env', importFormat: 'Import format', importContent: 'Paste .env or JSON content', importPreview: 'Preview fields', confirmImport: 'Confirm and encrypt import', imported: 'Imported and encrypted' },
    form: { addTitle: 'Add secret', editTitle: 'Edit secret', name: 'Name', envKey: 'Environment key', provider: 'Provider', value: 'Secret value', notes: 'Notes', tags: 'Tags (comma separated)', cancel: 'Cancel', save: 'Save secret', required: 'Required' },
    security: { localOnly: 'Stored on this device', encrypted: 'AES-256-GCM encrypted', audit: 'Audited without plaintext', locked: 'Vault is locked', unlock: 'Unlock vault', warning: 'Security note: secret values never enter browser storage, logs or AI context.' },
    auth: { setupTitle: 'Create local vault', unlockTitle: 'Unlock SecretHub', subtitle: 'Your Master Password stays on this device and cannot be recovered by a server.', password: 'Master Password', confirmPassword: 'Confirm password', setup: 'Create and unlock', unlock: 'Unlock vault', minLength: 'At least 12 characters', mismatch: 'The passwords do not match' },
    settings: { title: 'Settings', general: 'General', security: 'Security', ai: 'AI / MCP', theme: 'Theme', defaultExport: 'Default export file', autoLock: 'Auto lock', requireAuth: 'Require verification before export', language: 'Display language', backup: 'Export encrypted backup', backupSaved: 'Encrypted backup written to the selected folder', on: 'On', off: 'Off' },
    profile: { title: 'Profiles', name: 'Profile name', save: 'Save profile', apply: 'Apply profile', saved: 'Profile saved', empty: 'No profiles yet' },
    project: { title: 'Project history', recent: 'Recently used projects', record: 'Record current project', empty: 'No project history yet' },
    auditPanel: { title: 'Audit log', operation: 'Operation', result: 'Result', empty: 'No audit events yet' },
    mcp: { title: 'AI request awaiting approval', description: 'MCP requested a project environment plan. Review the masked preview before writing.', confirm: 'Approve export', confirmed: 'Approved and exported' },
    footer: { version: 'V1.0 MVP', vault: 'Local vault', synced: 'Protected' },
  },
};

export function getTranslation(locale: Locale | undefined): Translation {
  return translations[locale ?? 'zh-CN'];
}

export function readStoredLocale(value: string | null): Locale {
  return value === 'en-US' ? 'en-US' : 'zh-CN';
}
