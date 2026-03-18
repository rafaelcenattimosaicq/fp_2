/* eslint-disable no-var */

import { fetchAuthSession } from 'aws-amplify/auth';
import { invoke } from '@tauri-apps/api/core';

export async function getToken(): Promise<string> {
    const res1 = await fetchAuthSession();
    const token = res1.tokens?.idToken?.toString() ?? '';

    if (token !== '') {
      return token
    }


    var response = await invoke<string>('get_auth_token');
    // console.log('got token from tauri backend');
    return response;
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
