import { render, screen, waitFor } from '@testing-library/react';
import { describe, it, expect, vi, beforeEach } from 'vitest';

class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver = ResizeObserverStub as unknown as typeof ResizeObserver;

import App from './App';

const mockClient = {
  on: vi.fn(),
  subscribe: vi.fn(),
  unsubscribe: vi.fn(),
  publish: vi.fn(),
  end: vi.fn(),
  connected: false,
};

vi.mock('mqtt', () => ({
  default: { connect: vi.fn(() => mockClient) },
  connect: vi.fn(() => mockClient),
}));

vi.mock('echarts/core', () => {
  const mockChart = {
    setOption: vi.fn(),
    resize: vi.fn(),
    dispose: vi.fn(),
    showLoading: vi.fn(),
    hideLoading: vi.fn(),
  };
  return {
    init: vi.fn(() => mockChart),
    getInstanceByDom: vi.fn(() => mockChart),
    use: vi.fn(),
  };
});

vi.stubGlobal('fetch', vi.fn((url: string) =>
  Promise.resolve({
    ok: true,
    
    json: () => {
      if (typeof url === 'string' && (url.includes('/policies') || url.includes('/devices'))) {
        return Promise.resolve([]);
      }
      return Promise.resolve({ sources: [] });
    },
  } as Response),
));

vi.mock('@uiw/react-codemirror', () => ({
  default: ({ value, onChange }: { value: string; onChange?: (val: string) => void }) => (
    <textarea data-testid="codemirror" value={value} onChange={(e) => onChange?.(e.target.value)} />
  ),
}));
vi.mock('@codemirror/lang-yaml', () => ({ yaml: () => [] }));
vi.mock('@uiw/codemirror-theme-vscode', () => ({ vscodeLight: [] }));

vi.mock('aws-amplify/auth', () => ({
  fetchAuthSession: () =>
    Promise.resolve({
      tokens: { idToken: { toString: () => 'fake-token' } },
    }),
}));

const authState = { status: 'authenticated' as string };
vi.mock('./contexts/AuthContext', async () => {
  const { createContext } = await import('react');
  return {
    
    AuthContext: createContext(null),
    AuthProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
    useAuth: () => ({
      user: authState.status === 'authenticated' ? {} : null,
      status: authState.status,
      groups: [],
      signIn: vi.fn(),
      confirmMfa: vi.fn(),
      completeNewPassword: vi.fn(),
      signOut: vi.fn(),
    }),
  };
});

describe('App', () => {
  
  beforeEach(() => {
    window.history.pushState({}, '', '/');
    authState.status = 'authenticated';
  });

  it('renders the dashboard when authenticated', () => {
    render(<App />);
    expect(screen.getByText('Aura')).toBeInTheDocument();
  });

  it('redirects to login', () => {
    authState.status = 'unauthenticated';
    render(<App />);
    expect(screen.getByText('Enter your credentials to access Aura')).toBeInTheDocument();
  });

  it('shows loading state', () => {
    authState.status = 'loading';
    render(<App />);
    expect(screen.getByText('Loading...')).toBeInTheDocument();
  });

  it('shows Devices page', async () => {
    window.history.pushState({}, '', '/devices');
    render(<App />);
    
    await waitFor(() => {
      expect(screen.getByText(/select a policy or create a new one/i)).toBeInTheDocument();
    });
  });
});
