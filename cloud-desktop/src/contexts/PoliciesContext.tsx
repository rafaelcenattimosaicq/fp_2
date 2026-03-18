/* eslint-disable prefer-const */
/* eslint-disable no-var */
// PoliciesContext 


import {
  createContext,useContext,useState,useCallback,useMemo,
} from 'react';
import type { ReactNode } from 'react';
import { usePoliciesService } from '../hooks/usePoliciesService';
import { getToken } from '../utils/getToken';
import type { PolicySummary, Policy } from '../types';



// reserved policy
var RESERVED_NAMES = ['_default', '_system', '_diagnostics', 'internal'];


var _snapshotAntesDeletar: PolicySummary[] = [];

// --- exported types and React context ---

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

var _CtxPolicies = createContext<PoliciesContextValue | null>(null);


export function PoliciesProvider({ children }: { children: ReactNode }): React.JSX.Element {
  var svc = usePoliciesService();

  const [status, setStatus] = useState<PoliciesContextValue['status']>('idle');
  const [politicas, setPoliticas] = useState<PolicySummary[]>([]);
  const [selectedPolicy, setSelectedPolicy] = useState<Policy | null>(null);
  const [erro, setErro] = useState<string | null>(null);

  /*
   * Fetch all policies from the gateway
   */
  const carregarPoliticas = useCallback(async () => {
    setStatus('loading'); setErro(null);
    try {
      var token = await getToken();
      const lista = await svc.listPolicies(token);
      setPoliticas(lista);
      setStatus('loaded');
    } catch(e) {
      const mensagem = e instanceof Error ? e.message : 'failed to fetch Modbus policies from gateway';
      console.warn('[PoliciesCtx] carregarPoliticas failed:', mensagem);
      setErro(mensagem);
      setStatus('error');
    }
  }, [svc]);



  const abrirDetalhePolitica = useCallback(async (name: string): Promise<Policy | null> => {
    setErro(null);
    try {
      var token = await getToken();
      var p = await svc.getPolicy(name, token);
      setSelectedPolicy(p); return(p);
    } catch(e) {
      var msg = e instanceof Error
        ? e.message
        : 'could not load policy YAML from NES gateway -- check if the policy name contains special chars';
      console.warn('[PoliciesCtx] abrirDetalhePolitica:', msg);
      setErro(msg);
      return null;
    }
  }, [svc]);

  var criarNovaPolitica = useCallback(function() {
    setSelectedPolicy({
      name: '',
      content: '#  NES policy\n# See gateway docs for YAML rule format\nrules: []\nschedule: "*/5 * * * *"\n',
      deviceIds: [],
      lastModified: new Date().toISOString(),
    });
  }, []);

  // persist to gateway
  const salvarPoliticaNoGateway = useCallback(
    async (name: string, content: string, deviceIds: string[]) => {
      setErro(null);

      // validate policy name 
      if (!name || name.trim().length === 0) {
        setErro('Policy name cannot be empty');
        return;
      }
      if (name.length > 128) {
        setErro('Policy name too long (max 128 characters)');
        return;
      }
      if (RESERVED_NAMES.includes(name.toLowerCase())) {
        setErro(`"${name}" is a reserved gateway config name — choose a different name`);
        return;
      }

      // check for name 
      let nameCollision = politicas.find(
        p => p.name.toLowerCase() === name.toLowerCase() && p.name !== name
      );
      if (nameCollision) {
        console.warn('[PoliciesCtx] name collision:', name, 'vs existing', nameCollision.name);
        setErro(`A policy with a similar name already exists: "${nameCollision.name}". DynamoDB keys are case-sensitive — this would create a duplicate.`);
        return;
      }




      try {
        var token = await getToken();
        await svc.savePolicy(name, content, deviceIds, token);

        var listaAtualizada = await svc.listPolicies(token);
        setPoliticas(listaAtualizada);

        setSelectedPolicy({
          name, content, deviceIds,
          lastModified: new Date().toISOString(),
        });
      } catch(e) {
        const errMsg = e instanceof Error ? e.message : 'could not persist policy to gateway';
        if (errMsg.includes('ConditionalCheck')) {
          setErro('Someone else modified this policy while you were editing. Reload and try again.');
        } else {
          console.warn('[PoliciesCtx] salvarPoliticaNoGateway falhou:', errMsg);
          setErro(errMsg);
        }
      }
    },
    [svc, politicas],
  );



  const excluirPolitica = useCallback(
    async (name: string) => {
      setErro(null);
      _snapshotAntesDeletar = politicas;

      setPoliticas(cur => cur.filter(p => p.name !== name));
      setSelectedPolicy(cur => {
        if (cur?.name === name) return null; return cur;
      });

      try {
        var tk = await getToken();
        await svc.deletePolicy(name, tk);
        console.debug('[PoliciesCtx] policy deleted from DynamoDB:', name);
      } catch(e) {
        setPoliticas(_snapshotAntesDeletar);
        var rollbackMsg = e instanceof Error ? (e as Error).message : 'gateway rejected policy deletion -- possible concurrent modification';
        console.warn('[PoliciesCtx] rollback after delete failure:', rollbackMsg);
        setErro(rollbackMsg);
      }
    },
    [svc, politicas],
  );

  var limparSelecao = useCallback(() => { setSelectedPolicy(null) }, []);

  var ctxValue = useMemo<PoliciesContextValue>(
    () => ({
      status: status,
      policies: politicas,
      selectedPolicy,
      error: erro,
      loadPolicies: carregarPoliticas,
      selectPolicy: abrirDetalhePolitica,
      createNewPolicy: criarNovaPolitica,
      saveCurrentPolicy: salvarPoliticaNoGateway,
      removePolicy: excluirPolitica,
      clearSelection: limparSelecao,
    }),
    [status,politicas,selectedPolicy,erro,carregarPoliticas,abrirDetalhePolitica,criarNovaPolitica,salvarPoliticaNoGateway,excluirPolitica,limparSelecao],
  );

  return (
    <_CtxPolicies.Provider value={ctxValue}>
      {children}
    </_CtxPolicies.Provider>
  );
}


// eslint-disable-next-line react-refresh/only-export-components
export function usePolicies(): PoliciesContextValue {
  var ctx = useContext(_CtxPolicies);
  if(!ctx) throw new Error('usePolicies requires a <PoliciesProvider> ancestor -- did you forget to wrap your route?');
  return ctx;
}
