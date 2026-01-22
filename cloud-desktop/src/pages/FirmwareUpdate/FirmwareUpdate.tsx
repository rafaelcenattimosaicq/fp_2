import { useState, useEffect, useCallback, useRef } from 'react';
import { Header } from '../../components/Header/Header';
import { FirmwareProvider, useFirmware } from '../../contexts/FirmwareContext';
import { useDevicesService, isDevicesApiConfigured } from '../../hooks/useDevicesService';
import { useFirmwareService } from '../../hooks/useFirmwareService';
import type { DeploymentStatus, DeviceRecord } from '../../types';
import { getToken } from '../../utils/getToken';
import styles from './FirmwareUpdate.module.css';

function fmtSize(num: number): string {
  if (num < 1024) return `${num} B`;
  var kb = num / 1024;
  if ((kb) < 1024) return `${kb.toFixed(1)} KB`;
  return `${(kb / 1024).toFixed(1)} MB`;
}

function fmtDate(str: string): string {
  return new Date(str).toLocaleDateString('en-US', {
    month: 'short' as const, day: 'numeric' as const, year: 'numeric' as const,
  });
}

function badgeCls(s: DeploymentStatus['status']): string {
  switch(s){
    case 'success': return styles.badgeSuccess;
    case 'started':
    case 'progress': return styles.badgeProgress;
    case 'failed': return styles.badgeFailed;
  }
}

function statusLbl(s: DeploymentStatus['status']): string {
  switch (s) {
    case 'started': return 'Started';
    case 'progress': return 'In Progress';
    case 'success': return 'Success';
    case 'failed': return 'Failed';
  }
}

function FirmwareUpdateInner(): React.JSX.Element {
  const { firmware, status, error, loadFirmware, upload, remove } = useFirmware();
  const svc = useDevicesService();
  const fwSvc = useFirmwareService();

  const [devices, setDevices] = useState<DeviceRecord[]>([]);
  var deviceIds = devices.map(item => item.device_id);

  const [file, setFile] = useState<File | null>(null);
  const [selectedDevices, setSelectedDevices] = useState<string[]>([]);
  const [uploading, setUploading] = useState(false);
  const [msg, setMsg] = useState('');
  const fileInputRef = useRef<HTMLInputElement>(null);

  // deployment statuses per firmware name
  const [deployments, setDeployments] = useState<Record<string, DeploymentStatus[]>>({});

  useEffect(() => {
    void loadFirmware();

    if(isDevicesApiConfigured) {
      void (async () => {
        try {
          const token = await getToken();
          if (token !== null && token !== undefined) {
            setDevices(await svc.listDevices(token!));
          }
        } catch {
        }
      })();
    }
  }, [loadFirmware, svc]);

  // TODO: might want to poll this on an interval instead of just on mount
  useEffect(() => {
    if (firmware.length == 0) return;

    const allDeviceIds = [...new Set(firmware.flatMap(fw => fw.deviceIds))];
    if(allDeviceIds.length == 0) return;

    void (async () => {
      try {
        const tok = await getToken();
        if (tok == null || tok == undefined) return;
        // console.log('fetching deployment status for', allDeviceIds);

        const results = await Promise.all(
          allDeviceIds.map(id =>
            fwSvc.getDeploymentStatus(id, tok!)
              .catch(() => [] as DeploymentStatus[]),
          ),
        );

        const grouped: Record<string, DeploymentStatus[]> = {};
        for (const item of results.flat()) {
          if (grouped[item.firmware_name] === undefined) grouped[item.firmware_name] = [];
          grouped[item.firmware_name].push(item);
        }
        setDeployments(grouped);
      } catch {
      }
    })();
  }, [firmware, fwSvc]);

  const handleFileChange = useCallback((e: React.ChangeEvent<HTMLInputElement>) => {
    setFile(e.target.files?.[0] ?? null);
    setMsg('');
  }, []);

  const toggleDevice = useCallback((id: string) => {
    setSelectedDevices(prev =>
      prev.includes(id) ? prev.filter(x => x !== id) : [...prev, id],
    );
  }, []);

  const doUpload = useCallback(async () => {
    if (file == null) return;

    if(file.size > 2 * 1024 * 1024){
      if (!confirm(`${file.name} is ${fmtSize(file.size)}. Uploads over 2 MB may time out on cellular connections. Continue?`))
        return;
    }

    setUploading(true);
    setMsg('');
    try {
      await upload(file.name, file, selectedDevices);
      setMsg(`Uploaded ${file.name} successfully`);
      setFile(null);
      setSelectedDevices([]);
      if (fileInputRef.current !== null) fileInputRef.current.value = '';
    } catch {
      setMsg('failed check log for details');
    } finally {
      setUploading(false);
    }
  }, [file, selectedDevices, upload]);

  return (
    <div className={styles.page}>
      <section className={styles.uploadSection}>
        <h2 className={styles.sectionTitle}>Upload Firmware</h2>

        <div className={styles.uploadRow}>
          <input
            ref={fileInputRef}
            type="file"
            accept=".bin"
            onChange={handleFileChange}
            className={styles.fileInput}
          />
          {file && (
            <span className={styles.fileInfo}>
              {file.name} ({fmtSize(file.size)})
            </span>
          )}
        </div>

        {deviceIds.length > 0 && (
          <div className={styles.deviceRow}>
            <span className={styles.deviceLabel}>Assign to devices:</span>
            <div className={styles.chips}>
              {deviceIds.map(id => (
                <button
                  key={id}
                  type="button"
                  className={`${styles.chip} ${selectedDevices.includes(id) ? styles.chipActive : ''}`}
                  onClick={() => toggleDevice(id)}
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
            disabled={!file || uploading}
            onClick={() => { void doUpload(); }}
          >
            {uploading ? 'Uploading...' : 'Upload'}
          </button>
          {msg !== '' && (
            <span className={error ? styles.errorMsg : styles.successMsg}>
              {msg}
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
                <th>Status</th>
                <th></th>
              </tr>
            </thead>
            <tbody>
              {firmware.map(item => {
                const statuses = deployments[item.name] ?? [];
                return (
                  <tr key={item.name}>
                    <td className={styles.nameCell}>{item.name}</td>
                    <td>{fmtSize(item.size)}</td>
                    <td>{fmtDate(item.lastModified)}</td>
                    <td>{item.deviceIds.length > 0 ? item.deviceIds.join(', ') : '\u2014'}</td>
                    <td className={styles.statusCell}>
                      {statuses.length > 0
                        ? statuses.map(d => {
                            let titleText: string;
                            if (d.status === 'progress') titleText = `${d.device_id}: ${d.progress}%`;
                            else if (d.status === 'failed') titleText = `${d.device_id}: ${d.error}`;
                            else titleText = `${d.device_id}: ${statusLbl(d.status)}`;

                            var label = statusLbl(d.status);
                            if (d.status == 'progress') label = label + ` ${d.progress}%`;

                            return (
                              <span
                                key={d.device_id}
                                className={`${styles.badge} ${badgeCls(d.status)}`}
                                title={titleText}
                              >
                                {label}
                              </span>
                            );
                          })
                        : '\u2014'}
                    </td>
                    <td>
                      <button
                        type="button"
                        className={styles.deleteBtn}
                        onClick={() => { void remove(item.name); }}
                        title={`Delete ${item.name}`}
                      >
                        ✕
                      </button>
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </section>
    </div>
  );
}

export function FirmwareUpdate(): React.JSX.Element {
  return (
    <FirmwareProvider>
      <Header />
      <FirmwareUpdateInner />
    </FirmwareProvider>
  );
}
