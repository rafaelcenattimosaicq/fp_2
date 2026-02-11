import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { MemoryRouter } from 'react-router';
import { Login } from './Login';
import type { SignInResult, AuthContextValue } from '../contexts/AuthContext';

const mockSignIn = vi.fn<(email: string, password: string) => Promise<SignInResult>>();
const mockConfirmMfa = vi.fn<(code: string) => Promise<void>>();
const mockCompleteNewPassword = vi.fn<(newPassword: string) => Promise<SignInResult>>();
const mockSignOut = vi.fn();

vi.mock('../contexts/AuthContext', () => ({
  useAuth: (): AuthContextValue => ({
    user: null,
    status: 'unauthenticated',
    groups: [],
    signIn: mockSignIn,
    confirmMfa: mockConfirmMfa,
    completeNewPassword: mockCompleteNewPassword,
    signOut: mockSignOut,
  }),
}));

vi.mock('aws-amplify/auth', () => ({
  signOut: vi.fn().mockResolvedValue(undefined),
}));

function renderLogin(): void {
  render(
    <MemoryRouter initialEntries={['/login']}>
      <Login />
    </MemoryRouter>,
  );
}

describe('Login', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('renders the credentials form', () => {
    renderLogin();

    expect(screen.getByLabelText('Email')).toBeInTheDocument();
    expect(screen.getByLabelText('Password')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Sign in' })).toBeInTheDocument();
    expect(screen.getByText('Enter your credentials to access Aura')).toBeInTheDocument();
  });

  it('submits credentials and transitions to MFA step', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'MFA_REQUIRED' });

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(mockSignIn).toHaveBeenCalledWith('test@example.com', 'password123');
    
    expect(screen.getByLabelText('MFA Code')).toBeInTheDocument();
    expect(screen.getByText('Enter the 6-digit code from your authenticator app')).toBeInTheDocument();
  });

  it('goes to dashboard', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'SUCCESS' });

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(mockSignIn).toHaveBeenCalledWith('test@example.com', 'password123');
  });

  it('submits MFA code successfully', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'MFA_REQUIRED' });
    mockConfirmMfa.mockResolvedValue(undefined);

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    await user.type(screen.getByLabelText('MFA Code'), '654321');
    await user.click(screen.getByRole('button', { name: 'Verify' }));

    expect(mockConfirmMfa).toHaveBeenCalledWith('654321');
  });

  it('displays error message on sign-in failure', async () => {
    const user = userEvent.setup();
    mockSignIn.mockRejectedValue(new Error('Incorrect username or password'));

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'wrong');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(screen.getByRole('alert')).toHaveTextContent(
      'Incorrect username or password',
    );
  });

  it('displays error on MFA failure', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'MFA_REQUIRED' });
    mockConfirmMfa.mockRejectedValue(new Error('Invalid code'));

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    await user.type(screen.getByLabelText('MFA Code'), '000000');
    await user.click(screen.getByRole('button', { name: 'Verify' }));

    expect(screen.getByRole('alert')).toHaveTextContent('Invalid code');
  });

  it('navigates back from MFA to credentials', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'MFA_REQUIRED' });

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(screen.getByLabelText('MFA Code')).toBeInTheDocument();

    await user.click(screen.getByText('Back to sign in'));

    expect(screen.getByLabelText('Email')).toBeInTheDocument();
    expect(screen.getByLabelText('Password')).toBeInTheDocument();
  });

  it('renders email input with type="email" and password input with type="password"', () => {
    renderLogin();

    const emailInput = screen.getByLabelText('Email');
    const passwordInput = screen.getByLabelText('Password');

    expect(emailInput).toHaveAttribute('type', 'email');
    expect(passwordInput).toHaveAttribute('type', 'password');
  });

  it('shows "Signing in..." loading state during authentication', async () => {
    const user = userEvent.setup();
    
    let resolveSignIn!: (value: SignInResult) => void;
    mockSignIn.mockImplementation(
      () => new Promise<SignInResult>((resolve) => { resolveSignIn = resolve; }),
    );

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(screen.getByRole('button', { name: 'Signing in...' })).toBeInTheDocument();
    
    expect(screen.getByRole('button', { name: 'Signing in...' })).toBeDisabled();

    resolveSignIn({ step: 'SUCCESS' });
  });

  it('disables sign-in button', async () => {
    const user = userEvent.setup();
    let resolveSignIn!: (value: SignInResult) => void;
    mockSignIn.mockImplementation(
      () => new Promise<SignInResult>((resolve) => { resolveSignIn = resolve; }),
    );

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(screen.getByRole('button', { name: 'Signing in...' })).toBeDisabled();

    resolveSignIn({ step: 'SUCCESS' });
  });

  it('shows generic error message when signIn rejects with a non-Error value', async () => {
    const user = userEvent.setup();
    mockSignIn.mockRejectedValue('string error');

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(screen.getByRole('alert')).toHaveTextContent('Sign in failed');
  });

  it('transitions to new password step when required', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'NEW_PASSWORD_REQUIRED' });

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'temppass');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(screen.getByText('Set new password')).toBeInTheDocument();
    expect(screen.getByText('Your temporary password must be changed')).toBeInTheDocument();
    expect(screen.getByLabelText('New Password')).toBeInTheDocument();
    expect(screen.getByLabelText('Confirm Password')).toBeInTheDocument();
  });

  it('shows error when new passwords', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'NEW_PASSWORD_REQUIRED' });

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'temppass');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    await user.type(screen.getByLabelText('New Password'), 'newpass123');
    await user.type(screen.getByLabelText('Confirm Password'), 'different1');
    await user.click(screen.getByRole('button', { name: 'Set password' }));

    expect(screen.getByRole('alert')).toHaveTextContent('Passwords do not match');
    
    expect(mockCompleteNewPassword).not.toHaveBeenCalled();
  });

  it('shows error when new password is shorter than 8 characters', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'NEW_PASSWORD_REQUIRED' });

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'temppass');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    await user.type(screen.getByLabelText('New Password'), 'short');
    await user.type(screen.getByLabelText('Confirm Password'), 'short');
    await user.click(screen.getByRole('button', { name: 'Set password' }));

    expect(screen.getByRole('alert')).toHaveTextContent('Password must be at least 8 characters');
    expect(mockCompleteNewPassword).not.toHaveBeenCalled();
  });

  it('transitions to MFA setup step with QR code', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({
      step: 'MFA_SETUP_REQUIRED',
      setupUri: 'otpauth://totp/CloudDesktop:test@example.com?secret=ABCDEFGH',
      sharedSecret: 'ABCDEFGH',
    });

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    expect(screen.getByText('Set up authenticator')).toBeInTheDocument();
    expect(screen.getByText('ABCDEFGH')).toBeInTheDocument();
    expect(screen.getByLabelText('Verification code')).toBeInTheDocument();
  });

  it('renders the progress indicator bar with Credentials, Password, and MFA labels', () => {
    renderLogin();

    expect(screen.getByText('Credentials')).toBeInTheDocument();
    
    const passwordElements = screen.getAllByText('Password');
    expect(passwordElements.length).toBeGreaterThanOrEqual(2);
    expect(screen.getByText('MFA')).toBeInTheDocument();
  });

  it('renders the Aura branding ribbon', () => {
    renderLogin();

    expect(screen.getByText('Aura')).toBeInTheDocument();
    
    const signInElements = screen.getAllByText('Sign in');
    expect(signInElements.length).toBeGreaterThanOrEqual(2);
  });

  it('clears error when navigating back from new password step', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'NEW_PASSWORD_REQUIRED' });

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'temppass');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    await user.type(screen.getByLabelText('New Password'), 'newpass123');
    await user.type(screen.getByLabelText('Confirm Password'), 'different1');
    await user.click(screen.getByRole('button', { name: 'Set password' }));
    expect(screen.getByRole('alert')).toBeInTheDocument();

    await user.click(screen.getByText('Back to sign in'));
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('resets to credentials on session', async () => {
    const user = userEvent.setup();
    mockSignIn.mockResolvedValue({ step: 'MFA_REQUIRED' });
    mockConfirmMfa.mockRejectedValue(new Error('signIn was not called'));

    renderLogin();

    await user.type(screen.getByLabelText('Email'), 'test@example.com');
    await user.type(screen.getByLabelText('Password'), 'password123');
    await user.click(screen.getByRole('button', { name: 'Sign in' }));

    await user.type(screen.getByLabelText('MFA Code'), '123456');
    await user.click(screen.getByRole('button', { name: 'Verify' }));

    expect(screen.getByRole('alert')).toHaveTextContent('Session expired');
    expect(screen.getByLabelText('Email')).toBeInTheDocument();
  });
});
