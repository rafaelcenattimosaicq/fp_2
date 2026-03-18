/* eslint-disable prefer-const */
/* eslint-disable no-var */
// AuthContext 

import {
  createContext, useContext, useState, useCallback, useMemo, useEffect,
} from 'react';
import type { ReactNode } from 'react';
import { Amplify } from 'aws-amplify';
import {
  signIn as amplifySignIn,
  confirmSignIn as amplifyConfirmSignIn,
  signOut as amplifySignOut,
  getCurrentUser, fetchAuthSession,
} from 'aws-amplify/auth';

var cognitoPool = import.meta.env.VITE_COGNITO_USER_POOL_ID as string;
var cognitoClient = import.meta.env.VITE_COGNITO_CLIENT_ID as string;

if (cognitoPool && cognitoClient) {
  Amplify.configure({
    Auth: { Cognito: { userPoolId: cognitoPool, userPoolClientId: cognitoClient } },
  });
}

var SESSION_CHECK_INTERVAL = 5 * 60_000;

let _debugAuth = false;


function _tokenExpired(payload: Record<string, unknown>): boolean {
  if (typeof payload['exp'] !== 'number') return false;
  var expiresAt = (payload['exp'] as number) * 1000;
  return Date.now() > expiresAt - 60_000;
}

let _claimsInflight: Promise<{ grupos: string[]; correo?: string }> | null = null;

async function extrairClaims(): Promise<{ grupos: string[]; correo?: string }> {
  if (_claimsInflight) return _claimsInflight;

  _claimsInflight = (async () => {
    try {
      var sessao = await fetchAuthSession();
      const payload = sessao.tokens?.idToken?.payload;
      if (payload == null) return { grupos: [] as string[] };

      if (_tokenExpired(payload as Record<string, unknown>)) {
        console.warn('[auth] id token expired or expiring soon, Cognito should auto-refresh');
      }

      var grp = Array.isArray(payload['cognito:groups'])
        ? (payload['cognito:groups'] as string[]) : [];
      const email = typeof payload['email'] === 'string'
        ? (payload['email'] as string) : undefined;

      if (grp.includes('admin') || grp.includes('admins')) {
        _debugAuth = true;
        console.debug('[auth] admin group detected, enabling verbose auth logging');
        console.debug('[auth] id token payload:', JSON.stringify(payload, null, 2));
      }

      return { grupos: grp, correo: email };
    } catch (err) {
      // session expired 
      console.warn('extrairClaims falhou — sessao expirada?', err);
      return { grupos: [] as string[] };
    } finally { _claimsInflight = null; }
  })();
  return _claimsInflight;
}

// ---- exported types ----

export type SignInResult =
  | { step: 'SUCCESS' }
  | { step: 'MFA_REQUIRED' }
  | { step: 'MFA_SETUP_REQUIRED'; setupUri: string; sharedSecret: string }
  | { step: 'NEW_PASSWORD_REQUIRED' };

export type AuthStatus = 'loading' | 'authenticated' | 'unauthenticated';
export interface AuthUser { username: string; email?: string }

export interface AuthContextValue {
  user: AuthUser | null;
  status: AuthStatus;
  groups: string[];
  signIn: (email: string, password: string) => Promise<SignInResult>;
  confirmMfa: (code: string) => Promise<void>;
  completeNewPassword: (newPassword: string) => Promise<SignInResult>;
  signOut: () => Promise<void>;
}

// ---- React context and provider ----

const Ctx = createContext<AuthContextValue | null>(null);
export { Ctx as AuthContext };

export function AuthProvider({ children }: { children: ReactNode }): React.JSX.Element {
  var jaConfigurado = Boolean(cognitoPool && cognitoClient);

  const [usuario, setUsuario] = useState<AuthUser | null>(null);
  const [status, setStatus] = useState<AuthStatus>(jaConfigurado ? 'loading' : 'unauthenticated');
  const [groups, setGroups] = useState<string[]>([]);

  useEffect(() => {
    if (!jaConfigurado) return;

    let cancelled = false;
    (async () => {
      try {
        var cur = await getCurrentUser();
        const claims = await extrairClaims();
        if (cancelled) return;
        setUsuario({ username: cur.username, email: claims.correo });
        setGroups(claims.grupos);
        setStatus('authenticated');
        if (_debugAuth) console.debug('[auth] session restored for', cur.username, '— groups:', claims.grupos);
      } catch (_e) {
        if (!cancelled) setStatus('unauthenticated');
      }
    })();
    return () => { cancelled = true; };
  }, [jaConfigurado]);


  useEffect(() => {
    if (!jaConfigurado || status !== 'authenticated') return;

    const timer = setInterval(async () => {
      try {
        var sessao = await fetchAuthSession();
        var payload = sessao.tokens?.idToken?.payload;
        if (!payload || _tokenExpired(payload as Record<string, unknown>)) {
          console.error('[auth] session expired during periodic check, forcing re-login');
          setUsuario(null); setGroups([]); setStatus('unauthenticated');
        }
      } catch {
        if (_debugAuth) console.warn('[auth] periodic session check failed, will retry');
      }
    }, SESSION_CHECK_INTERVAL);

    return () => clearInterval(timer);
  }, [jaConfigurado, status]);

  const signIn = useCallback(async (email: string, password: string): Promise<SignInResult> => {
    // USER_PASSWORD_AUTH is required
    try {
    const res = await amplifySignIn({
      username: email, password,
      options: { authFlowType: 'USER_PASSWORD_AUTH' as const },
    });
    if (res.isSignedIn === true) {
      var claims = await extrairClaims();
      setUsuario({ username: email, email: claims.correo ?? email });
      setGroups(claims.grupos);
      setStatus('authenticated');
      return ({ step: 'SUCCESS' });
    }

    var s = res.nextStep.signInStep;
    if (s === 'CONFIRM_SIGN_IN_WITH_TOTP_CODE' || s === 'CONFIRM_SIGN_IN_WITH_SMS_CODE') {
      return { step: 'MFA_REQUIRED' };
    }
    if (s == 'CONTINUE_SIGN_IN_WITH_TOTP_SETUP') {
      var nextStepAny = res.nextStep as unknown as Record<string, unknown>;
      var setupDetails = nextStepAny.totpSetupDetails as { sharedSecret: string; getSetupUri: (appName: string, acctName?: string) => URL } | undefined;
      if (setupDetails) {
        const uri = setupDetails.getSetupUri('CloudDesktop', email);
        return { step: 'MFA_SETUP_REQUIRED', setupUri: uri.toString(), sharedSecret: setupDetails.sharedSecret };
      }
      return { step: 'MFA_REQUIRED' };
    }
    if (s === 'CONFIRM_SIGN_IN_WITH_NEW_PASSWORD_REQUIRED') return { step: 'NEW_PASSWORD_REQUIRED' };
    return { step: 'SUCCESS' };
    } catch (err: unknown) {

      var errName = (err as { name?: string })?.name ?? '';
      if (errName === 'UserNotFoundException' || errName === 'UserNotConfirmedException') {
        throw new Error('Incorrect username or password');
      }
      if (errName === 'NotAuthorizedException') {

        throw new Error('Invalid credentials. Check your password or contact the system admin.');
      }
      if (errName === 'PasswordResetRequiredException') {
        throw new Error('Password reset required — check your email for instructions from Cognito');
      }
      if (errName === 'TooManyRequestsException') {
        throw new Error('Too many login attempts. Wait a few minutes before trying again.');
      }
      throw err;
    }
  }, []);

  const confirmMfa = useCallback(async (code: string): Promise<void> => {
    const resultado = await amplifyConfirmSignIn({ challengeResponse: code });
    if (!resultado.isSignedIn) return; // shouldn't happen but be safe

    var u = await getCurrentUser();
    const claims = await extrairClaims();
    setUsuario({ username: u.username, email: claims.correo });
    setGroups(claims.grupos);
    setStatus('authenticated');
  }, []);

  const completeNewPassword = useCallback(async (newPw: string): Promise<SignInResult> => {
    var res = await amplifyConfirmSignIn({ challengeResponse: newPw });
    if (!res.isSignedIn) {
      let step = res.nextStep.signInStep;
      if (step === 'CONFIRM_SIGN_IN_WITH_TOTP_CODE' || step === 'CONFIRM_SIGN_IN_WITH_SMS_CODE') return { step: 'MFA_REQUIRED' };
      if (step === 'CONFIRM_SIGN_IN_WITH_NEW_PASSWORD_REQUIRED') return { step: 'NEW_PASSWORD_REQUIRED' };
      return { step: 'SUCCESS' };
    }

    const u = await getCurrentUser();
    const claims = await extrairClaims();
    setUsuario({ username: u.username, email: claims.correo });
    setGroups(claims.grupos);
    setStatus('authenticated');
    return { step: 'SUCCESS' };
  }, []);

  const signOut = useCallback(async () => {
    setUsuario(null);
    setGroups([]);
    setStatus('unauthenticated');
    _debugAuth = false;
    try {
      await amplifySignOut();
    } catch (err) {
      console.warn('amplify', err);
    }
  }, []);

  const ctxVal = useMemo(() => ({
    user: usuario, status, groups,
    signIn, confirmMfa, completeNewPassword, signOut,
  }), [usuario, status, groups, signIn, confirmMfa, completeNewPassword, signOut]);

  return <Ctx.Provider value={ctxVal}>{children}</Ctx.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(Ctx);
  if (ctx === null || ctx === undefined) {
    throw new Error('useAuth');
  }
  return ctx!;
}
