/*
 * Login */
const EMBRACO_MIN_PW_LENGTH = 12;

type LoginStep = 'credentials' | 'newPassword' | 'mfaSetup' | 'mfa';

function getTitle(s: LoginStep) {
  if(s === 'credentials') return 'Sign in';
  if(s === 'newPassword') return 'Set new password';
  if(s === 'mfaSetup') return 'Set up authenticator';
  return 'Verification';
}

function getSubtitle(s: LoginStep): string {
  switch(s){
    case 'credentials': return 'Enter your credentials to access Aura';
    case 'newPassword': return 'Your temporary password must be changed';
    case 'mfaSetup': return 'Scan the QR code or enter the secret in your authenticator app';
    case 'mfa': return 'Enter the 6-digit code from your authenticator app';
  }
}

export function meetsPasswordPolicy(pw: string): boolean {
  if (pw.length < EMBRACO_MIN_PW_LENGTH) return false;
  let hasUpper = false, hasLower = false, hasDigit = false;
  for (const ch of pw) {
    if (ch >= 'A' && ch <= 'Z') hasUpper = true;
    if (ch >= 'a' && ch <= 'z') hasLower = true;
    if (ch >= '0' && ch <= '9') hasDigit = true;
  }
  return hasUpper && hasLower && hasDigit;
}

import { useState } from 'react';
import type { FormEvent } from 'react';
import { useNavigate } from 'react-router';
import { QRCodeSVG } from 'qrcode.react';
import {signOut as amplifySignOut} from 'aws-amplify/auth';
import { useAuth } from '../contexts/AuthContext';
import styles from '../styles/login.module.css';

export function Login(): React.JSX.Element {
  const { signIn, confirmMfa, completeNewPassword, status } = useAuth();
  const navigate = useNavigate();

  const [step, setStep] = useState<LoginStep>('credentials');
  const [email, setEmail] = useState('');
  const [password, setPassword] = useState('');
  const [newPw, setNewPw] = useState('');
  const [confirmPw, setConfirmPw] = useState('');
  const [mfaCode, setMfaCode] = useState('');
  const [sharedSecret, setSharedSecret] = useState('');
  const [setupUri, setSetupUri] = useState('');
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  if (status === 'authenticated') {
    navigate('/', { replace: true });
    return <div className={styles.page} />;
  }

  // took a while to figure out the amplify sign-out dance
  async function handleCredentials(e: FormEvent): Promise<void> {
    e.preventDefault();
    setError(null);
    setLoading(true);
    // console.log('attempting login for:', email);

    try {
      try { await amplifySignOut(); } catch { }

      const result = await signIn(email, password);

      if (result.step === 'NEW_PASSWORD_REQUIRED') {
        setStep('newPassword');
      } else if (result.step == 'MFA_SETUP_REQUIRED') {
        setSharedSecret(result.sharedSecret);
        setSetupUri(result.setupUri);
        setStep('mfaSetup');
      } else if (result.step === 'MFA_REQUIRED') {
        setStep('mfa');
      } else {
        navigate('/', {replace: true});
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Sign in failed');
    } finally {
      setLoading(false);
    }
  }

  async function handleNewPassword(e: FormEvent): Promise<void> {
    e.preventDefault();
    setError(null);

    if(newPw !== confirmPw) {
      setError('Passwords do not match');
      return;
    }
    if ((newPw.length) < 8) {
      setError('Password must be at least 8 characters');
      return;
    }

    setLoading(true);
    try {
      const result = await completeNewPassword(newPw);
      if (result.step === 'MFA_SETUP_REQUIRED') {
        setSharedSecret(result.sharedSecret);
        setSetupUri(result.setupUri);
        setStep('mfaSetup');
      } else if (result.step == 'MFA_REQUIRED') {
        setStep('mfa');
      } else {
        navigate('/', { replace: true });
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to set password');
    } finally {
      setLoading(false);
    }
  }

  async function handleMfa(e: FormEvent): Promise<void> {
    e.preventDefault();
    setError(null);
    setLoading(true);
    try {
      await confirmMfa(mfaCode);
      navigate('/', { replace: true });
    } catch(err) {
      const msg = err instanceof Error ? err.message : 'Invalid MFA code';

      if (msg.includes('signIn was not called') || msg.includes('session has expired') || msg.includes('CodeMismatchException')) {
        setStep('credentials');
        setMfaCode('');
        setSharedSecret('');
        setSetupUri('');
        setPassword('');
        setError('Session expired - please sign in again');
      } else {
        setError(msg);
      }
    } finally {
      setLoading(false);
    }
  }

  function goBack(): void {
    setStep('credentials');
    setMfaCode('');
    setNewPw('');
    setConfirmPw('');
    setError(null);
  }

  return (
    <div className={styles.page}>
      <div className={styles.ribbon}>
        <div className={styles.ribbonIcon}>
          <span /><span /><span />
          <span /><span /><span />
          <span /><span /><span />
        </div>
        <span className={styles.ribbonTitle}>Aura</span>
      </div>

      <div className={styles.cardWrap}>
        <div className={styles.card}>
          <div className={styles.cardAccent} />
          <div className={styles.cardBody}>
            <h1 className={styles.title}>{getTitle(step)}</h1>
            <p className={styles.subtitle}>{getSubtitle(step)}</p>

            {error !== null && (
              <div className={styles.error} role="alert">
                {error}
              </div>
            )}

            {step === 'credentials' && (
              <form className={styles.form} onSubmit={handleCredentials}>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="email">
                    Email
                  </label>
                  <input id="email" className={styles.input} type="email"
                    value={email} onChange={(e) => setEmail(e.target.value)}
                    placeholder="you@example.com" required autoComplete="username"
                  />
                </div>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="password">Password</label>
                  <input
                    id="password"
                    className={styles.input}
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="Enter your password"
                    required
                    autoComplete="current-password"
                  />
                </div>

                <button className={styles.button} type="submit" disabled={loading}>
                  {loading ? 'Signing in...' : 'Sign in'}
                </button>
              </form>
            )}

            {step == 'newPassword' && (
              <form className={styles.form} onSubmit={handleNewPassword}>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="new-password">New Password</label>
                  <input id="new-password" className={styles.input} type="password"
                    value={newPw} onChange={(e) => setNewPw(e.target.value)}
                    placeholder="At least 8 characters" required minLength={8}
                    autoComplete="new-password" />
                </div>

                <div className={styles.field}>
                  <label className={styles.label} htmlFor="confirm-password">
                    Confirm Password
                  </label>
                  <input
                    id="confirm-password"
                    className={styles.input}
                    type="password"
                    value={confirmPw}
                    onChange={(e) => setConfirmPw(e.target.value)}
                    placeholder="Repeat your new password"
                    required
                    minLength={8}
                    autoComplete="new-password"
                  />
                </div>
                <button className={styles.button} type="submit" disabled={loading}>
                  {loading ? 'Setting password...' : 'Set password'}
                </button>
                <button type="button" className={styles.backLink} onClick={goBack}>
                  Back to sign in
                </button>
              </form>
            )}

            {step === 'mfaSetup' && (
              <form className={styles.form} onSubmit={handleMfa}>
                <div className={styles.qrWrap}>
                  <div className={styles.qrBox}>
                    <QRCodeSVG value={setupUri} size={180}
                      bgColor="#ffffff" fgColor="#1a1a1a" level="M" />
                  </div>
                </div>
                <div className={styles.field}>
                  <label className={styles.label}>Or enter manually</label>
                  <div className={styles.secretBox}>
                    <code className={styles.secretCode}>{sharedSecret}</code>
                  </div>
                </div>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="setup-code">Verification code</label>
                  <input id="setup-code" className={styles.mfaInput} type="text"
                    inputMode="numeric" pattern="[0-9]{6}" maxLength={6}
                    value={mfaCode} onChange={(e) => setMfaCode(e.target.value)}
                    placeholder="000000" required autoComplete="one-time-code" />
                </div>
                <button className={styles.button} type="submit" disabled={loading}>
                  {loading ? 'Verifying...' : 'Verify & complete setup'}
                </button>

                <button type="button" className={styles.backLink} onClick={goBack}>
                  Back to sign in
                </button>
              </form>
            )}

            {step === 'mfa' && (
              <form className={styles.form} onSubmit={handleMfa}>
                <div className={styles.field}>
                  <label className={styles.label} htmlFor="mfa-code">MFA Code</label>
                  <input
                    id="mfa-code" className={styles.mfaInput}
                    type="text" inputMode="numeric"
                    pattern="[0-9]{6}" maxLength={6}
                    value={mfaCode}
                    onChange={(e) => setMfaCode(e.target.value)}
                    placeholder="000000" required
                    autoComplete="one-time-code"
                  />
                </div>
                <button className={styles.button} type="submit" disabled={loading}>
                  {loading ? 'Verifying...' : 'Verify'}
                </button>
                <button type="button" className={styles.backLink} onClick={goBack}>Back to sign in</button>
              </form>
            )}
          </div>
        </div>
      </div>

      <div className={styles.statusBar}>
        <span className={styles.statusItem}>
          <span className={styles.statusDot} data-ready={step === 'credentials' ? 'true' : 'false'} />
          Credentials
        </span>
        <span className={styles.statusItem}>
          <span className={styles.statusDot} data-ready={step === 'newPassword' ? 'true' : 'false'} />
          Password
        </span>
        <span className={styles.statusItem}>
          <span className={styles.statusDot} data-ready={step === 'mfaSetup' || step === 'mfa' ? 'true' : 'false'} />
          MFA
        </span>
      </div>
    </div>
  );
}
