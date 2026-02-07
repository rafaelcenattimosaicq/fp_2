
import { createHmac } from 'node:crypto';
import { Amplify } from 'aws-amplify';
import {
  signIn,
  confirmSignIn,
  fetchAuthSession,
} from 'aws-amplify/auth';

let cachedToken: string | null = null;

const USER_POOL_ID = import.meta.env.VITE_COGNITO_USER_POOL_ID as string;
const CLIENT_ID = import.meta.env.VITE_COGNITO_CLIENT_ID as string;
const TEST_USER = import.meta.env.VITE_TEST_USER as string;
const TEST_PASSWORD = import.meta.env.VITE_TEST_PASSWORD as string;
const TOTP_SECRET = import.meta.env.VITE_TEST_TOTP_SECRET as string;

function generateTotpCode(secretB32: string): string {
  
  const alphabet = 'ABCDEFGHIJKLMNOPQRSTUVWXYZ234567';
  let bits = '';
  for (const ch of secretB32.toUpperCase()) {
    const idx = alphabet.indexOf(ch);
    if (idx === -1) continue;
    bits += idx.toString(2).padStart(5, '0');
  }
  const key = Buffer.from(
    bits.match(/.{8}/g)?.map((b) => parseInt(b, 2)) ?? [],
  );

  const counter = Math.floor(Date.now() / 1000 / 30);
  const counterBuf = Buffer.alloc(8);
  counterBuf.writeUInt32BE(Math.floor(counter / 0x100000000), 0);
  counterBuf.writeUInt32BE(counter & 0xffffffff, 4);

  const hmac = createHmac('sha1', key).update(counterBuf).digest();
  const offset = hmac[hmac.length - 1] & 0x0f;
  const code =
    ((hmac[offset] & 0x7f) << 24) |
    ((hmac[offset + 1] & 0xff) << 16) |
    ((hmac[offset + 2] & 0xff) << 8) |
    (hmac[offset + 3] & 0xff);

  return String(code % 1_000_000).padStart(6, '0');
}

export async function getTestToken(): Promise<string> {
  if (cachedToken) return cachedToken;

  if (!USER_POOL_ID || !CLIENT_ID) {
    throw new Error(
      'Missing VITE_COGNITO_USER_POOL_ID or VITE_COGNITO_CLIENT_ID - check your .env file.',
    );
  }
  if (!TEST_USER || !TEST_PASSWORD) {
    throw new Error(
      'Missing VITE_TEST_USER or VITE_TEST_PASSWORD - add test credentials to your .env file.',
    );
  }
  if (!TOTP_SECRET) {
    throw new Error(
      'Missing VITE_TEST_TOTP_SECRET - add the TOTP secret to your .env file.',
    );
  }

  Amplify.configure({
    Auth: {
      Cognito: {
        userPoolId: USER_POOL_ID,
        userPoolClientId: CLIENT_ID,
      },
    },
  });

  const result = await signIn({
    username: TEST_USER,
    password: TEST_PASSWORD,
    options: { authFlowType: 'USER_PASSWORD_AUTH' },
  });

  if (!result.isSignedIn) {
    const step = result.nextStep.signInStep;

    if (
      step === 'CONFIRM_SIGN_IN_WITH_TOTP_CODE' ||
      step === 'CONFIRM_SIGN_IN_WITH_SMS_CODE'
    ) {
      const totpCode = generateTotpCode(TOTP_SECRET);
      const mfaResult = await confirmSignIn({ challengeResponse: totpCode });

      if (!mfaResult.isSignedIn) {
        throw new Error(
          `MFA confirmation did not complete - next step: ${mfaResult.nextStep.signInStep}.`,
        );
      }
    } else {
      throw new Error(
        `Sign-in did not complete - unexpected step: ${step}. ` +
        'Ensure the test user has completed MFA setup.',
      );
    }
  }

  const session = await fetchAuthSession();
  const accessToken = session.tokens?.accessToken?.toString();

  if (!accessToken) {
    throw new Error('Signed in but no acc...');
  }

  cachedToken = accessToken;
  return cachedToken;
}
