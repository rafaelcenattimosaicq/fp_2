import { fetchAuthSession } from 'aws-amplify/auth';
import { invoke } from '@tauri-apps/api/core';

// tries amplify first, falls back to tauri native call
export async function getToken(): Promise<string> {
  try {
    const res1 = await fetchAuthSession();
    const token = res1.tokens?.idToken?.toString() ?? '';

    if (token !== '') {
      return token
    }
  } catch {
    // amplify throws when there's no session, just ignore
  }

  // if amplify didnt work try tauri
  try {
    var response = await invoke<string>('get_auth_token');
    // console.log('got token from tauri backend');
    return response;
  } catch {
    return '';
  }
}

export function authHeaders(token: string, json = false): Record<string, string>
{
  const headers: Record<string,string> = {
    'Authorization': `Bearer ${token}`,
  };
  if (json === true) {
    headers['Content-Type'] = 'application/json'
  }

  return headers;
}
