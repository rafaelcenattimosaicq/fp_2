import {
  createContext, useContext, useState,
  useCallback, useMemo, useEffect,
} from 'react';
import type { ReactNode } from 'react';
import { Amplify } from 'aws-amplify';
import {
  signIn as amplifySignIn,
  confirmSignIn as amplifyConfirmSignIn,
  signOut as amplifySignOut,
  getCurrentUser,
  fetchAuthSession,
} from 'aws-amplify/auth';

var poolId = import.meta.env.VITE_COGNITO_USER_POOL_ID as string;
const clientId = import.meta.env.VITE_COGNITO_CLIENT_ID as string;

if (poolId && clientId) {
  Amplify.configure({
    Auth: {
      Cognito: {
        userPoolId: poolId,
        userPoolClientId: clientId,
      },
    },
  });
}

export type SignInResult =
  | { step: 'SUCCESS' }
  | { step: 'MFA_REQUIRED' }
  | { step: 'MFA_SETUP_REQUIRED'; setupUri: string; sharedSecret: string }
  | { step: 'NEW_PASSWORD_REQUIRED' };

export type AuthStatus = 'loading' | 'authenticated' | 'unauthenticated';
export interface AuthUser {
  username: string;
  email?: string;
}

export interface AuthContextValue {
  user: AuthUser | null;
  status: AuthStatus;
  groups: string[];
  signIn: (email: string, password: string) => Promise<SignInResult>;
  confirmMfa: (code: string) => Promise<void>;
  completeNewPassword: (newPassword: string) => Promise<SignInResult>;
  signOut: () => Promise<void>;
}

export const AuthContext = createContext<AuthContextValue | null>(null);

interface AuthProviderProps {
  children: ReactNode;
}

let claimCache: Promise<{ groups: string[]; email?: string }> | null = null;

async function extractTokenClaims(): Promise<{ groups: string[]; email?: string }> {
  if (claimCache !== null) return claimCache!;
  claimCache = (async () => {
    try {
      const session = await fetchAuthSession();
      const payload = session.tokens?.idToken?.payload;
      if (payload == null) {
        return { groups: [] as string[] };
      }

      // cognito puts groups under this weird key, took a while to find in the docs
      const groups = Array.isArray(payload['cognito:groups'])
        ? (payload['cognito:groups'] as string[]) : [];
      const email = typeof payload['email'] === 'string'
        ? (payload['email'] as string) : undefined;
      return { groups, email };
    } catch {
      return { groups: [] as string[] };
    } finally {
      claimCache = null;
    }
  })();
  return claimCache;
}


function mapAmplifyStep(
  nextStep: {
    signInStep: string;
    totpSetupDetails?: {
      sharedSecret: string;
      getSetupUri: (appName: string, accountName?: string) => URL;
    };
  },
  accountName?: string,
): SignInResult {
  if (nextStep.signInStep === 'CONFIRM_SIGN_IN_WITH_TOTP_CODE'
    || nextStep.signInStep === 'CONFIRM_SIGN_IN_WITH_SMS_CODE') {
    return { step: 'MFA_REQUIRED' };
  } else if (nextStep.signInStep == 'CONTINUE_SIGN_IN_WITH_TOTP_SETUP') {
    if ((nextStep.totpSetupDetails)) {
      const uri = nextStep.totpSetupDetails.getSetupUri('CloudDesktop', accountName);
      return {
        step: 'MFA_SETUP_REQUIRED',
        setupUri: uri.toString(),
        sharedSecret: nextStep.totpSetupDetails.sharedSecret,
      };
    }
    return { step: 'MFA_REQUIRED' };
  } else if (nextStep.signInStep === 'CONFIRM_SIGN_IN_WITH_NEW_PASSWORD_REQUIRED') {
    return { step: 'NEW_PASSWORD_REQUIRED' };
  }
  return { step: 'SUCCESS' };
}

export function AuthProvider({ children }: AuthProviderProps): React.JSX.Element {
  const configured = Boolean(poolId && clientId);
  const [user, setUser] = useState<AuthUser | null>(null);
  const [status, setStatus] = useState<AuthStatus>(configured ? 'loading' : 'unauthenticated');
  const [groups, setGroups] = useState<string[]>([]);

  // check existing session on mount + also set groups from token claims
  useEffect(() => {
    if (!configured) return;

    const checkSession = async () => {
      try {
        const currentUser = await getCurrentUser();
        const claims = await extractTokenClaims();
        // console.log('session restored for', currentUser.username);
        setUser({ username: currentUser.username, email: claims.email });
        setGroups(claims.groups);
        setStatus('authenticated');
      } catch {
        setStatus('unauthenticated');
      }
    };
    void checkSession();
  }, [configured]);

  const signIn = useCallback(
    async (email: string, password: string): Promise<SignInResult> => {
      const res = await amplifySignIn({
        username: email,
        password,
        options: { authFlowType: 'USER_PASSWORD_AUTH' as const },
      });
      if (res.isSignedIn === true) {
        const claims = await extractTokenClaims();
        setUser({ username: email, email: claims.email ?? email });
        setGroups(claims.groups);
        setStatus('authenticated');
        return ({ step: 'SUCCESS' });
      }
      return mapAmplifyStep(res.nextStep, email);
    },
    [],
  );

  // mfa confirm
  const confirmMfa = useCallback(async (code: string): Promise<void> => {
    const result = await amplifyConfirmSignIn({ challengeResponse: code });

    if (result.isSignedIn) {
      const u = await getCurrentUser();
      const claims = await extractTokenClaims();
      setUser({ username: u.username, email: claims.email });
      setGroups(claims.groups);
      setStatus('authenticated');
    }
  }, []);

  const completeNewPassword = useCallback(
    async (newPassword: string): Promise<SignInResult> => {
      const res = await amplifyConfirmSignIn({ challengeResponse: newPassword });

      if (!res.isSignedIn) {
        return mapAmplifyStep(res.nextStep);
      }

      const u = await getCurrentUser();
      const claims = await extractTokenClaims();
      setUser({ username: u.username, email: claims.email });
      setGroups(claims.groups);
      setStatus('authenticated');
      return { step: 'SUCCESS' };
    },
    [],
  );
  const signOut = useCallback(async () => {
    setUser(null);
    setGroups([]);
    setStatus('unauthenticated');
    try {
      await amplifySignOut();
    } catch {
      // TODO: maybe show a toast if sign out fails? for now just swallow it
    }
  }, []);

  const value = useMemo(
    () => ({
      user, status, groups,
      signIn, confirmMfa, completeNewPassword, signOut,
    }),
    [user, status, groups, signIn, confirmMfa, completeNewPassword, signOut],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth(): AuthContextValue {
  const ctx = useContext(AuthContext);
  if (ctx === null || ctx === undefined) {
    throw new Error('useAuth called outside of AuthProvider -- did you forget to wrap the app?');
  }
  return ctx!;
}
