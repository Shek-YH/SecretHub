import { fireEvent, render, screen } from '@testing-library/react';
import { afterEach, expect, it, vi } from 'vitest';
import { AboutLinks } from './AboutLinks';

afterEach(() => vi.restoreAllMocks());

it('links to the Chinese site pages', () => {
  const open = vi.spyOn(window, 'open').mockReturnValue(null);
  render(<AboutLinks locale="zh-CN" />);
  fireEvent.click(screen.getByText('隐私政策 ↗'));
  expect(open).toHaveBeenCalledWith('https://secrethub.bigstoneopc.com/zh/privacy/', '_blank', 'noopener,noreferrer');
});

it('links to the English site pages', () => {
  const open = vi.spyOn(window, 'open').mockReturnValue(null);
  render(<AboutLinks locale="en-US" />);
  fireEvent.click(screen.getByText('Website ↗'));
  expect(open).toHaveBeenCalledWith('https://secrethub.bigstoneopc.com/', '_blank', 'noopener,noreferrer');
});
