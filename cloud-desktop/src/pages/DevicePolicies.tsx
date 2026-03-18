/* eslint-disable no-var */

export var EMBRACO_OTA_PARTITION_LIMIT = 1900 * 1024; // bytes, approximate

function fmtSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  var kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

// Dates displayed in the policy list and firmware catalogue
function fmtDate(iso: string): string {
  return new Date(iso).toLocaleDateString('en-US', {
    month: 'short', day: 'numeric', year: 'numeric',
  });
}

export const VALID_POLL_INTERVALS = [1, 2, 5, 10, 30, 60] as const;

export function isValidPolicyYaml(raw: string): boolean {
  try {
    const doc = (typeof raw === 'string') ? raw.trim() : '';
    return doc.length > 0 && doc.startsWith('version');
  } catch {
    return false;
  }
}

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

type DevicesTab = 'policies' | 'firmware' | 'configure';

function PoliciesTab(): React.JSX.Element {
  const {
    policies, selectedPolicy, loadPolicies,
    selectPolicy, createNewPolicy, saveCurrentPolicy, removePolicy,
  } = usePolicies();

  const devSvc = useDevicesService();

  const [editName, setEditName] = useState('');
  const [editContent, setEditContent] = useState('');
  const [editDeviceIds, setEditDeviceIds] = useState<string[]>([]);
  const [saveStatus, setSaveStatus] = useState<SaveStatus>('saved');
  const [selecting, setSelecting] = useState(false);

  const deviceOwnership = useMemo(() => {
    const map: Record<string, string> = {};
    for (const p of policies) {
      if (p.name === editName) continue;
      for (const id of p.deviceIds) map[id] = p.name;
    }
    return map;
  }, [policies, editName]);

  const [regDevices, setRegDevices] = useState<DeviceRecord[]>([]);
  const [devicesLoading, setDevicesLoading] = useState(false);
  const [devicesErr, setDevicesErr] = useState<string | null>(null);

  const loadDevices = useCallback(async () => {
    if (!isDevicesApiConfigured) return;
    setDevicesLoading(true);
    setDevicesErr(null);
    try {
      const tok = await getToken();
      if (!tok) return;
      const devs = await devSvc.listDevices(tok);
      setRegDevices(devs);
    } catch (e) {
      setDevicesErr(String(e));
    } finally {
      setDevicesLoading(false);
    }
  }, [devSvc]);

  useEffect(() => {
    void loadPolicies();
    void loadDevices();
  }, [loadPolicies, loadDevices]);

  async function handleAddDevice(deviceId: string, protocol: string, icon?: string): Promise<void> {
    setDevicesLoading(true);
    setDevicesErr(null);
    try {
      const tok = await getToken();
      await devSvc.saveDevice(deviceId, protocol, tok, icon);
      await loadDevices();
    } catch (e) {
      setDevicesErr(`Failed to add device: ${String(e)}`);
    } finally {
      setDevicesLoading(false);
    }
  }

  async function handleDeleteDevice(deviceId: string): Promise<void> {
    setDevicesLoading(true);
    setDevicesErr(null);
    try {
      const tok = await getToken();
      await devSvc.deleteDevice(deviceId, tok);
      await loadDevices();
    } catch (e) {
      setDevicesErr(`Failed to delete device: ${String(e)}`);
    } finally {
      setDevicesLoading(false);
    }
  }

  const handleSelect = useCallback(async (name: string) => {
    setSelecting(true);
    try {
      const policy = await selectPolicy(name);
      if (policy) {
        setEditName(policy.name);
        setEditContent(policy.content);
        setEditDeviceIds(policy.deviceIds);
        setSaveStatus('saved');
      }
    } finally {
      setSelecting(false);
    }
  }, [selectPolicy]);

  function handleNew(): void {
    createNewPolicy();
    setEditName('');
    setEditContent('# new policy\nrules: []\n');
    setEditDeviceIds([]);
    setSaveStatus('saved');
  }

  function handleDelete(name: string): void {
    void removePolicy(name);
    if (editName === name) {
      setEditName('');
      setEditContent('');
      setEditDeviceIds([]);
      setSaveStatus('saved');
    }
  }

  const handleSave = useCallback(async () => {
    if (!editName.trim()) return;
    try {
      yaml.load(editContent);
    } catch (e) {
      const yamlErr = e instanceof Error ? e.message : 'Invalid YAML';
      setSaveStatus('unsaved');
      alert(`YAML syntax error:\n${yamlErr}`);
      return;
    }
    setSaveStatus('saving');
    try {
      await saveCurrentPolicy(editName, editContent, editDeviceIds);
      setSaveStatus('saved');
    } catch {
      setSaveStatus('unsaved');
    }
  }, [editName, editContent, editDeviceIds, saveCurrentPolicy]);

  return (
    <div className={styles.layout}>
      <div className={styles.sidebar}>
        <div className={styles.registrySection}>
          <DeviceRegistry
            devices={regDevices}
            onAdd={(id, proto, icon) => { void handleAddDevice(id, proto, icon); }}
            onDelete={(id) => { void handleDeleteDevice(id); }}
            loading={devicesLoading}
          />
          {devicesErr && <p className={styles.errorMsg}>{devicesErr}</p>}
        </div>
        <div className={styles.policySection}>
          <PolicyList
            policies={policies}
            selectedName={selectedPolicy?.name ?? null}
            onSelect={(name) => { void handleSelect(name); }}
            onNew={handleNew}
            onDelete={handleDelete}
          />
        </div>
      </div>

      <div className={styles.editor}>
        <PolicyEditor
          policy={selectedPolicy}
          name={editName}
          content={editContent}
          deviceIds={editDeviceIds}
          devices={regDevices}
          saveStatus={saveStatus}
          loading={selecting}
          onSave={() => { void handleSave(); }}
          onContentChange={(v) => { setEditContent(v); setSaveStatus('unsaved'); }}
          onNameChange={(v) => { setEditName(v); setSaveStatus('unsaved'); }}
          onToggleDevice={(devId) => {
            setEditDeviceIds(prev =>
              prev.includes(devId) ? prev.filter(id => id !== devId) : [...prev, devId],
            );
            setSaveStatus('unsaved');
          }}
          onUpload={(filename, content) => {
            setEditContent(content);
            setSaveStatus('unsaved');
            if (!editName.trim()) {
              setEditName(filename.replace(/\.ya?ml$/i, ''));
            }
          }}
          deviceOwnership={deviceOwnership}
        />
      </div>
    </div>
  );
}

// ── Firmware Tab ─────────────────────────────────────────────────────────────

function FirmwareTab(): React.JSX.Element {
  const { firmware, status, error, loadFirmware, upload, remove } = useFirmware();
  const devSvc = useDevicesService();

  const [regDevices, setRegDevices] = useState<DeviceRecord[]>([]);
  const devIds = regDevices.map(d => d.device_id);

  const [selFile, setSelFile] = useState<File | null>(null);
  const [assignedDevs, setAssignedDevs] = useState<string[]>([]);
  const [uploading, setUploading] = useState(false);
  const [uploadMsg, setUploadMsg] = useState('');
  const fileRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    void loadFirmware();

    if (isDevicesApiConfigured) {
      void (async () => {
        try {
          const tok = await getToken();
          if (tok) {
            const devs = await devSvc.listDevices(tok);
            setRegDevices(devs);
          }
        } catch { /* devices list is non-critical here */ }
      })();
    }
  }, [loadFirmware, devSvc]);

  async function doUpload(): Promise<void> {
    if (!selFile) return;
    setUploading(true);
    setUploadMsg('');
    try {
      await upload(selFile.name, selFile, assignedDevs);
      setUploadMsg(`Uploaded ${selFile.name} successfully`);
      setSelFile(null);
      setAssignedDevs([]);
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
          <input
            ref={fileRef}
            type="file"
            accept=".bin"
            onChange={(e) => { setSelFile(e.target.files?.[0] ?? null); setUploadMsg(''); }}
            className={styles.fileInput}
          />
          {selFile && (
            <span className={styles.fileInfo}>
              {selFile.name} ({fmtSize(selFile.size)})
            </span>
          )}
        </div>

        {devIds.length > 0 && (
          <div className={styles.deviceRow}>
            <span className={styles.deviceLabel}>Assign to devices:</span>
            <div className={styles.chips}>
              {devIds.map(id => (
                <button
                  key={id}
                  type="button"
                  className={`${styles.chip} ${assignedDevs.includes(id) ? styles.chipActive : ''}`}
                  onClick={() => setAssignedDevs(prev =>
                    prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id],
                  )}
                >
                  {id}
                </button>
              ))}
            </div>
          </div>
        )}

        <div className={styles.uploadActions}>
          <button
            type="button"
            className={styles.uploadBtn}
            disabled={!selFile || uploading}
            onClick={() => { void doUpload(); }}
          >
            {uploading ? 'Uploading...' : 'Upload'}
          </button>
          {uploadMsg && (
            <span className={error ? styles.errorMsg : styles.successMsg}>
              {uploadMsg}
            </span>
          )}
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
            <thead>
              <tr>
                <th>Name</th>
                <th>Size</th>
                <th>Uploaded</th>
                <th>Devices</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {firmware.map(fw => (
                <tr key={fw.name}>
                  <td className={styles.nameCell}>{fw.name}</td>
                  <td>{fmtSize(fw.size)}</td>
                  <td>{fmtDate(fw.lastModified)}</td>
                  <td>{fw.deviceIds.length > 0 ? fw.deviceIds.join(', ') : '\u2014'}</td>
                  <td>
                    <button
                      type="button"
                      className={styles.fwDeleteBtn}
                      onClick={() => { void remove(fw.name); }}
                      title={`Delete ${fw.name}`}
                    >
                      ✕
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
  const devSvc = useDevicesService();
  const { subscribe } = useMqtt();

  const [regDevices, setRegDevices] = useState<DeviceRecord[]>([]);
  const [activeGws, setActiveGws] = useState<string[]>([]);
  const seenGwsRef = useRef<Set<string>>(new Set());
  const [selDeviceId, setSelDeviceId] = useState('');
  const [gwId, setGwId] = useState('');
  const [params, setParams] = useState<DescriptorRegister[]>([]);
  const [cfgErr, setCfgErr] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  // fetch registered devices on mount
  useEffect(() => {
    if (!isDevicesApiConfigured) return;
    void (async () => {
      try {
        const tok = await getToken();
        if (!tok) return;
        const devs = await devSvc.listDevices(tok);
        setRegDevices(devs);
      } catch { /* non-critical */ }
    })();
  }, [devSvc]);

  useEffect(() => {
    const unsub = subscribe('controller_app/events', (_topic, payload) => {
      try {
        const msg = JSON.parse(payload) as Record<string, unknown>;
        const id = msg.GATEWAY_ID;
        if (typeof id === 'string' && id && !seenGwsRef.current.has(id)) {
          seenGwsRef.current.add(id);
          setActiveGws(Array.from(seenGwsRef.current));
        }
      } catch { /* malformed event, ignore */ }
    });
    return unsub;
  }, [subscribe]);

  async function loadDescriptor(deviceId: string): Promise<void> {
    setCfgErr(null);
    setParams([]);
    if (!deviceId) return;

    setLoading(true);
    try {
      const tok = await getToken();
      const device = await devSvc.getDevice(deviceId, tok);
      if (!device.descriptor) {
        setCfgErr('No descriptor assigned to this device.');
        return;
      }
      const desc = yaml.load(device.descriptor) as DeviceDescriptor;
      const writable = getWritableParameters(desc);
      if (writable.length === 0) {
        setCfgErr('Descriptor has no writable parameters (non-enum, non-bitwise).');
        return;
      }
      setParams(writable);
    } catch (e) {
      setCfgErr(e instanceof Error ? e.message : 'Failed to load descriptor');
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className={styles.configurePage}>
      <section className={styles.configureHeader}>
        <h2 className={styles.sectionTitle}>Remote Parameter Write</h2>

        <div className={styles.configureInputs}>
          <label className={styles.configureLabel}>
            Device
            <select
              className={styles.configureSelect}
              value={selDeviceId}
              onChange={(e) => {
                setSelDeviceId(e.target.value);
                void loadDescriptor(e.target.value);
              }}
            >
              <option value="">Select a device...</option>
              {regDevices.map(d => (
                <option key={d.device_id} value={d.device_id}>
                  {d.device_id}
                </option>
              ))}
            </select>
          </label>

          <label className={styles.configureLabel}>
            Gateway
            <select
              className={styles.configureSelect}
              value={gwId}
              onChange={(e) => setGwId(e.target.value)}
            >
              <option value="">
                {activeGws.length === 0 ? 'Waiting for telemetry...' : 'Select a gateway...'}
              </option>
              {activeGws.map(gw => (
                <option key={gw} value={gw}>{gw}</option>
              ))}
            </select>
          </label>
        </div>
      </section>

      {loading && <p className={styles.muted}>Loading descriptor...</p>}
      {cfgErr && <p className={styles.errorMsg}>{cfgErr}</p>}

      {params.length > 0 && gwId.trim() && (
        <DeviceConfigurator
          parameters={params}
          deviceId={selDeviceId}
          gatewayId={gwId.trim()}
        />
      )}

      {params.length > 0 && !gwId.trim() && (
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
        <button
          type="button"
          className={`${styles.tabBtn} ${tab === 'policies' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('policies')}
        >
          Policies
        </button>
        <button
          type="button"
          className={`${styles.tabBtn} ${tab === 'firmware' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('firmware')}
        >
          Firmware
        </button>
        <button
          type="button"
          className={`${styles.tabBtn} ${tab === 'configure' ? styles.tabBtnActive : ''}`}
          onClick={() => setTab('configure')}
        >
          Configure
        </button>
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
