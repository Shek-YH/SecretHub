import { render, screen } from '@testing-library/react';
import { App } from './App';

describe('SecretHub desktop shell', () => {
  it('starts in Chinese and exposes the language switch', () => {
    render(<App />);
    expect(screen.getByText('SecretHub')).toBeInTheDocument();
    expect(screen.getByText('凭据')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'English' })).toBeInTheDocument();
  });
});
