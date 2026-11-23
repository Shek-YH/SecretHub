import { backend, isTauriRuntime } from './lib/backend';
import type { Locale } from './i18n/translations';

const SITE = 'https://secrethub.bigstoneopc.com';

export function AboutLinks({ locale }: { locale: Locale }) {
  const zh = locale === 'zh-CN';
  const prefix = zh ? '/zh' : '';
  const links = [
    { label: zh ? '官网' : 'Website', url: `${SITE}${prefix}/` },
    { label: zh ? '隐私政策' : 'Privacy Policy', url: `${SITE}${prefix}/privacy/` },
    { label: zh ? '用户协议' : 'Terms of Use', url: `${SITE}${prefix}/terms/` },
    { label: zh ? '帮助与支持' : 'Support', url: `${SITE}${prefix}/support/` },
  ];
  const open = (url: string) => { if (isTauriRuntime()) void backend.openExternalUrl(url); else window.open(url, '_blank', 'noopener,noreferrer'); };
  return <div className="settings-footer about-links" aria-label={zh ? '关于' : 'About'}>
    {links.map((link) => <button key={link.url} type="button" className="inline-link" onClick={() => open(link.url)}>{link.label} ↗</button>)}
  </div>;
}
