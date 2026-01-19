import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { Header } from '../components/Header/Header';
import { PolicyList } from '../components/PolicyList/PolicyList';
import { PolicyEditor } from '../components/PolicyEditor/PolicyEditor';
import { DeviceRegistry } from '../components/DeviceRegistry/DeviceRegistry';
import type { SaveStatus } from '../components/PolicyEditor/PolicyEditor';
import { PoliciesProvider, usePolicies } from '../contexts/PoliciesContext';
import { FirmwareProvider, useFirmware } from '../contexts/FirmwareContext';
import { useDevicesService, isDevicesApiConfigured } from '../hooks/useDevicesService';
import { useMqtt } from '../contexts/MqttContext';
import yaml from 'js-yaml';
import type { DeviceRecord } from '../types';
import type { DeviceDescriptor, DescriptorRegister } from '../types/descriptor';
import { getWritableParameters } from '../types/descriptor';
import { DeviceConfigurator } from '../components/DeviceConfigurator/DeviceConfigurator';
import { getToken } from '../utils/getToken';
import styles from './DevicePolicies.module.css';
import { formatISO } from '../utils/formatDate';

type DevicesTab = 'policies' | 'firmware' | 'configure';

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes / 1024 < 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function fmtDate(iso: string): string { return formatISO(iso) }


// this whole tab ended up way bigger than I expected
function PoliciesTab(): React.JSX.Element {
  const {
    policies, selectedPolicy, loadPolicies,
    selectPolicy, createNewPolicy, saveCurrentPolicy, removePolicy,
  } = usePolicies();
  const svc = useDevicesService();

  const [policyName, setPolicyName] = useState('');
  const [policyContent, setPolicyContent] = useState('');
  const [deviceIds, setDeviceIds] = useState<string[]>([]);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('saved');
  const [selectLoading, setSelectLoading] = useState(false);

  const deviceOwnership = useMemo(() => {
    const map: Record<string, string> = {};
    for (const p of policies) {
      if (p.name === policyName) continue;
      for (const d of p.deviceIds) map[d] = p.name;
    }
    return map;
  }, [policies, policyName]);

  const [devices, setDevices] = useState<DeviceRecord[]>([]);
  const [devicesLoading, setDevicesLoading] = useState(false);
  const [devicesError, setDevicesError] = useState<string|null>(null);

  const fetchDevices = useCallback(async () => {
    if (!isDevicesApiConfigured) return;
    setDevicesLoading(true);
    setDevicesError(null);
    try {
      const token = await getToken();
      if (!token) return;
      // console.log('fetched devices count:', (await svc.listDevices(token)).length);
      setDevices(await svc.listDevices(token));
    } catch(e) {
      setDevicesError(String(e));
    } finally {
      setDevicesLoading(false);
    }
  }, [svc]);

  useEffect(() => {
    void loadPolicies();
    void fetchDevices();
  }, [loadPolicies, fetchDevices]);

  async function handleAddDevice(d: string, e: string, x?: string): Promise<void> {
    setDevicesLoading(true);
    setDevicesError(null);
    try {
      await svc.saveDevice(d, e, await getToken(), x);
      await fetchDevices();
    } catch(e) {
      setDevicesError(`Failed to add device: ${String(e)}`);
    } finally {
      setDevicesLoading(false);
    }
  }

  async function handleDeleteDevice(d: string): Promise<void> {
    setDevicesLoading(true);
    setDevicesError(null);
    try {
      await svc.deleteDevice(d, await getToken());
      await fetchDevices();
    } catch(e) {
      setDevicesError(`Failed to delete device: ${String(e)}`);
    } finally {
      setDevicesLoading(false);
    }
  }

  const handleSelectPolicy = useCallback(async (name: string) => {
    setSelectLoading(true);
    try {
      const res = await selectPolicy(name);
      if (res != null) {
        setPolicyName(res.name);
        setPolicyContent(res.content);
        setDeviceIds(res.deviceIds);
        setSaveStatus('saved');
      }
    } finally {
      setSelectLoading(false);
    }
  }, [selectPolicy]);

  function handleNewPolicy(): void {
    createNewPolicy();
    setPolicyName('');
    setPolicyContent('# new policy\nrules: []\n');
    setDeviceIds([]);
    setSaveStatus('saved');
  }

  function handleDeletePolicy(name: string) {
    void removePolicy(name);
    if (policyName === name) {
      setPolicyName(''); setPolicyContent(''); setDeviceIds([]); setSaveStatus('saved');
    }
  }

  const handleSave = useCallback(async () => {
    if (!policyName.trim()) return;
    try { yaml.load(policyContent); }
    catch (e) {
      setSaveStatus('unsaved');
      alert(`YAML syntax error:\n${e instanceof Error ? e.message : 'Invalid YAML'}`);
      return;
    }
    setSaveStatus('saving');
    try {
      await saveCurrentPolicy(policyName, policyContent, deviceIds);
      setSaveStatus('saved');
    } catch {
      setSaveStatus('unsaved');
    }
  }, [policyName, policyContent, deviceIds, saveCurrentPolicy]);

  return (
    <div className={styles.fullPage}>
      <div className={styles.devicesRow}>
        <DeviceRegistry
          devices={devices}
          onAdd={(id, proto, icon) => { void handleAddDevice(id, proto, icon); }}
          onDelete={(id) => { void handleDeleteDevice(id); }}
          loading={devicesLoading}
        />
        {devicesError && <p className={styles.errorMsg}>{devicesError}</p>}
      </div>

      <div className={styles.policyArea}>
        <div className={styles.policyCards}>
          <PolicyList
            policies={policies}
            selectedName={selectedPolicy?.name ?? null}
            onSelect={(name) => { void handleSelectPolicy(name); }}
            onNew={handleNewPolicy}
            onDelete={handleDeletePolicy}
          />
        </div>
        <div className={styles.editorArea}>
          <PolicyEditor
            policy={selectedPolicy}
            name={policyName}
            content={policyContent}
            deviceIds={deviceIds}
            devices={devices}
            saveStatus={saveStatus}
            loading={selectLoading}
            onSave={() => { void handleSave(); }}
            onContentChange={(c) => { setPolicyContent(c); setSaveStatus('unsaved'); }}
            onNameChange={(n) => { setPolicyName(n); setSaveStatus('unsaved'); }}
            onToggleDevice={(d) => {
              setDeviceIds(prev =>
                prev.includes(d) ? prev.filter(item => item !== d) : [...prev, d],
              );
              setSaveStatus('unsaved');
            }}
            onUpload={(filename, content) => {
              setPolicyContent(content);
              setSaveStatus('unsaved');
              if (!policyName.trim()) setPolicyName(filename.replace(/\.ya?ml$/i, ''));
            }}
            deviceOwnership={deviceOwnership}
          />
        </div>
      </div>
    </div>
  );
}

function FirmwareTab(): React.JSX.Element {
  const { firmware, status, error, loadFirmware, upload, remove } = useFirmware();
  const svc = useDevicesService();

  const [devices, setDevices] = useState<DeviceRecord[]>([]);
  const deviceIds = devices.map(d => d.device_id);

  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const [targetDevices, setTargetDevices] = useState<string[]>([]);
  const [uploading, setUploading] = useState(false);
  const [uploadMsg, setUploadMsg] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void loadFirmware();
    if (isDevicesApiConfigured) {
      void (async () => {
        try {
          const token = await getToken();
          if (token != null) setDevices(await svc.listDevices(token));
        } catch { }
      })();
    }
  }, [loadFirmware, svc]);

  async function handleUpload(): Promise<void> {
    if (selectedFile == null) return;
    setUploading(true);
    setUploadMsg('');
    try {
      await upload(selectedFile.name, selectedFile, targetDevices);
      setUploadMsg(`Uploaded ${selectedFile.name} successfully`);
      setSelectedFile(null);
      setTargetDevices([]);
      if (fileRef.current) fileRef.current.value = '';
    } catch {
      setUploadMsg('Upload failed, check the log for details');
    } finally {
      setUploading(false);
    }
  }

  return (
    <div className={styles.firmwarePage}>
      <section className={styles.uploadSection}>
        <h2 className={styles.sectionTitle}>Upload Firmware</h2>
        <div className={styles.uploadRow}>
          <input ref={fileRef} type="file" accept=".bin"
            onChange={e => { setSelectedFile(e.target.files?.[0] ?? null); setUploadMsg(''); }}
            className={styles.fileInput} />
          {selectedFile && (
            <span className={styles.fileInfo}>
              {selectedFile.name} ({fmtSize(selectedFile.size)})
            </span>
          )}
        </div>

        {deviceIds.length > 0 && (
          <div className={styles.deviceRow}>
            <span className={styles.deviceLabel}>Assign to devices:</span>
            <div className={styles.chips}>
              {deviceIds.map(id => (
                <button key={id} type="button"
                  className={`${styles.chip} ${targetDevices.includes(id) ? styles.chipActive : ''}`}
                  onClick={() => setTargetDevices(prev =>
                    prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id],
                  )}>
                  {id}
                </button>
              ))}
            </div>
          </div>
        )}

        <div className={styles.uploadActions}>
          <button type="button" className={styles.uploadBtn}
            disabled={!selectedFile || uploading}
            onClick={() => { void handleUpload(); }}>
            {uploading ? 'Uploading...' : 'Upload'}
          </button>
          {uploadMsg && <span className={error ? styles.errorMsg : styles.successMsg}>{uploadMsg}</span>}
        </div>
      </section>

      <section className={styles.listSection}>
        <h2 className={styles.sectionTitle}>Firmware Files</h2>

        {status === 'loading' && <p className={styles.muted}>Loading...</p>}
        {status === 'error' && <p className={styles.errorMsg}>{error}</p>}
        {status !== 'loading' && firmware.length === 0 && (
          <p className={styles.muted}>No firmware files uploaded yet.</p>
        )}

        {firmware.length > 0 && (
          <table className={styles.table}>
            <thead><tr>
              <th>Name</th><th>Size</th><th>Uploaded</th><th>Devices</th><th></th>
            </tr></thead>
            <tbody>
              {firmware.map(fw => (
                <tr key={fw.name}>
                  <td className={styles.nameCell}>{fw.name}</td>
                  <td>{fmtSize(fw.size)}</td>
                  <td>{fmtDate(fw.lastModified)}</td>
                  <td>{fw.deviceIds.length > 0 ? fw.deviceIds.join(', ') : '\u2014'}</td>
                  <td>
                    <button type="button" className={styles.fwDeleteBtn}
                      onClick={() => { void remove(fw.name); }}
                      title={`Delete ${fw.name}`}>
                      {'\u2715'}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}


function ConfigureTab(): React.JSX.Element {
  const svc = useDevicesService();
  const { subscribe } = useMqtt();

  const [devices, setDevices] = useState<DeviceRecord[]>([]);
  const [gateways, setGateways] = useState<string[]>([]);
  const seenGateways = useRef<Set<string>>(new Set());
  const [selectedDevice, setSelectedDevice] = useState('');
  const [selectedGateway, setSelectedGateway] = useState('');
  const [writableParams, setWritableParams] = useState<DescriptorRegister[]>([]);
  const [configError, setConfigError] = useState<string | null>(null);
  const [loadingDescriptor, setLoadingDescriptor] = useState(false);

  useEffect(() => {
    if (!isDevicesApiConfigured) return;
    void (async () => {
      try {
        const tok = await getToken();
        if (!tok) return;
        setDevices(await svc.listDevices(tok));
      } catch { }
    })();
  }, [svc]);

  // track gateways from mqtt events
  useEffect(() => {
    const unsub = subscribe('controller_app/events', (_topic, payload) => {
      try {
        const gwId = (JSON.parse(payload) as Record<string, unknown>).GATEWAY_ID;
        if (typeof gwId === 'string' && gwId && !seenGateways.current.has(gwId)) {
          seenGateways.current.add(gwId);
          setGateways(Array.from(seenGateways.current));
        }
      } catch { }
    });
    return unsub;
  }, [subscribe]);

  // finally got this working with the yaml descriptor parsing
  async function loadDescriptor(deviceId: string) {
    setConfigError(null);
    setWritableParams([]);
    if (!deviceId) return;
    setLoadingDescriptor(true);
    try {
      const device = await svc.getDevice(deviceId, await getToken());
      if (device.descriptor == null) {
        setConfigError('No descriptor assigned to this device.'); return;
      }
      const params = getWritableParameters(yaml.load(device.descriptor) as DeviceDescriptor);
      if (params.length === 0) {
        setConfigError('Descriptor has no writable parameters (non-enum, non-bitwise).'); return;
      }
      setWritableParams(params);
    } catch(e) {
      setConfigError(e instanceof Error ? e.message : 'Failed to load descriptor');
    } finally {
      setLoadingDescriptor(false);
    }
  }

  const gwTrimmed = selectedGateway.trim();

  return (
    <div className={styles.configurePage}>
      <section className={styles.configureHeader}>
        <h2 className={styles.sectionTitle}>Remote Parameter Write</h2>

        <div className={styles.configureInputs}>
          <label className={styles.configureLabel}>
            Device
            <select className={styles.configureSelect} value={selectedDevice}
              onChange={e => { setSelectedDevice(e.target.value); void loadDescriptor(e.target.value); }}>
              <option value="">Select a device...</option>
              {devices.map(d =>
                <option key={d.device_id} value={d.device_id}>{d.device_id}</option>
              )}
            </select>
          </label>

          <label className={styles.configureLabel}>
            Gateway
            <select
              className={styles.configureSelect}
              value={selectedGateway}
              onChange={(e) => setSelectedGateway(e.target.value)}
            >
              <option value="">
                {gateways.length === 0 ? 'Waiting for telemetry...' : 'Select a gateway...'}
              </option>
              {gateways.map(gw => (
                <option key={gw} value={gw}>{gw}</option>
              ))}
            </select>
          </label>
        </div>
      </section>

      {loadingDescriptor && <p className={styles.muted}>Loading descriptor...</p>}
      {configError && <p className={styles.errorMsg}>{configError}</p>}

      {writableParams.length > 0 && gwTrimmed !== '' && (
        <DeviceConfigurator parameters={writableParams} deviceId={selectedDevice} gatewayId={gwTrimmed} />
      )}
      {writableParams.length > 0 && gwTrimmed === '' && (
        <p className={styles.muted}>Select a gateway to enable parameter writes.</p>
      )}
    </div>
  );
}

function DevicePoliciesInner(): React.JSX.Element {
  const [tab, setTab] = useState<DevicesTab>('policies');

  return (
    <>
      <div className={styles.tabBar}>
        <button type="button"
          className={`${styles.tabBtn} ${tab === 'policies' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('policies')}>Policies</button>
        <button type="button"
          className={`${styles.tabBtn} ${tab === 'firmware' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('firmware')}>Firmware</button>
        <button type="button"
          className={`${styles.tabBtn} ${tab === 'configure' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('configure')}>Configure</button>
      </div>

      {tab === 'policies' && <PoliciesTab />}
      {tab === 'firmware' && <FirmwareTab />}
      {tab === 'configure' && <ConfigureTab />}
    </>
  );
}

function DevicePolicies(): React.JSX.Element {
  return (
    <PoliciesProvider>
      <FirmwareProvider>
        <Header />
        <DevicePoliciesInner />
      </FirmwareProvider>
    </PoliciesProvider>
  );
}

export default DevicePolicies;
