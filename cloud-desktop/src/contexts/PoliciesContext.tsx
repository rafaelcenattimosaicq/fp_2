import {
  createContext, useContext, useState, useCallback, useMemo,
} from 'react';
import type { ReactNode } from 'react';
import { usePoliciesService } from '../hooks/usePoliciesService';
import { getToken } from '../utils/getToken';
import type { PolicySummary, Policy } from '../types';

export interface PoliciesContextValue {
  status: 'idle' | 'loading' | 'loaded' | 'error';
  policies: PolicySummary[];
  selectedPolicy: Policy | null;
  error: string | null;
  loadPolicies: () => Promise<void>;
  selectPolicy: (name: string) => Promise<Policy | null>;
  createNewPolicy: () => void;
  saveCurrentPolicy: (name: string, content: string, deviceIds: string[]) => Promise<void>;
  removePolicy: (name: string) => Promise<void>;
  clearSelection: () => void;
}

export const PoliciesContext = createContext<PoliciesContextValue | null>(null);

export function PoliciesProvider({ children }: { children: ReactNode }): React.JSX.Element {
  const svc = usePoliciesService();
  const [status, setStatus] = useState<PoliciesContextValue['status']>('idle');
  const [policies, setPolicies] = useState<PolicySummary[]>([]);
  const [selectedPolicy, setSelectedPolicy] = useState<Policy | null>(null);
  const [error, setError] = useState<string | null>(null);

  const loadPolicies = useCallback(async () => {
    setStatus('loading');
    setError(null);
    try {
      const token = await getToken();
      setPolicies(await svc.listPolicies(token));
      setStatus('loaded');
    } catch(e) {
      setError(e instanceof Error ? e.message : 'could not load policies');
      setStatus('error');
    }
  }, [svc]);

  const selectPolicy = useCallback(async (name: string): Promise<Policy | null> => {
    setError(null);
    try {
      const token = await getToken();
      const p = await svc.getPolicy(name, token);
      setSelectedPolicy(p);
      return(p);
    } catch(e) {
      const msg = e instanceof Error ? e.message : 'load policy';
      setError(msg);
      return null;
    }
  }, [svc]);

  const createNewPolicy = useCallback(function() {
    // default YAML template for new edge policies
    setSelectedPolicy({
      name: '',
      content: '# new policy\nrules: []\n',
      deviceIds: [],
      lastModified: new Date().toISOString(),
    });
  }, []);


  const saveCurrentPolicy = useCallback(
    async (name: string, content: string, deviceIds: string[]) => {
      setError(null);
      try {
        const token = await getToken();
        await svc.savePolicy(name, content, deviceIds, token);
        // console.log('policy saved:', name, deviceIds.length, 'devices');
        setPolicies(await svc.listPolicies(token));
        setSelectedPolicy({ name, content, deviceIds, lastModified: new Date().toISOString() });
      } catch(e) {
        setError(e instanceof Error ? e.message : 'could not save policy');
      }
    },
    [svc],
  );

  // optimistic delete with rollback
  const removePolicy = useCallback(
    async (name: string) => {
      setError(null);
      const prevList = policies;
      setPolicies(cur => cur.filter(p => p.name !== name));
      setSelectedPolicy(cur => {
        if (cur?.name === name) return null;
        return cur;
      });

      try {
        await svc.deletePolicy(name, await getToken());
      } catch(e) {
        // rollback on failure - this took a while to get right
        setPolicies(prevList);
        setError(e instanceof Error ? (e as Error).message : 'could not delete policy');
      }
    },
    [svc, policies],
  );
  const clearSelection = useCallback(() => { setSelectedPolicy(null) }, []);

  const value = useMemo<PoliciesContextValue>(
    () => ({
      status,
      policies,
      selectedPolicy,
      error,
      loadPolicies,
      selectPolicy,
      createNewPolicy,
      saveCurrentPolicy,
      removePolicy,
      clearSelection,
    }),
    [status, policies, selectedPolicy, error, loadPolicies, selectPolicy, createNewPolicy, saveCurrentPolicy, removePolicy, clearSelection],
  );

  return (
    <PoliciesContext.Provider value={value}>
      {children}
    </PoliciesContext.Provider>
  );
}

// eslint-disable-next-line react-refresh/only-export-components
export function usePolicies(): PoliciesContextValue {
  const ctx = useContext(PoliciesContext);
  if(!ctx) throw new Error('usePolicies requires a <PoliciesProvider> ancestor');
  return ctx;
}
