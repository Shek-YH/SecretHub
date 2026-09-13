import { getTranslation, translations } from './translations';

describe('locale contract', () => {
  it('uses Chinese by default and has a complete English dictionary', () => {
    expect(getTranslation(undefined).nav.secrets).toBe('凭据');
    expect(translations['en-US'].nav.secrets).toBe('Secrets');
    expect(Object.keys(translations['zh-CN'])).toEqual(Object.keys(translations['en-US']));
  });
});
