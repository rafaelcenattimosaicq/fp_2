import { useRef, useCallback } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { yaml } from '@codemirror/lang-yaml';
import { vscodeLight } from '@uiw/codemirror-theme-vscode';
import type { Policy, DeviceRecord } from '../../types';
import styles from './PolicyEditor.module.css';

export type SaveStatus = 'saved' | 'unsaved' | 'saving';

interface Props {
    policy: Policy | null;
    name: string;
    content: string;
    deviceIds: string[];
    devices: DeviceRecord[];
    saveStatus: SaveStatus;
    loading?: boolean;
    onSave: () => void;
    onContentChange: (value: string) => void;
    onNameChange: (value: string) => void;
    onToggleDevice: (deviceId: string) => void;
    onUpload: (filename: string, content: string) => void;
    deviceOwnership?: Record<string, string>;
}

const STATUS_LBL: Record<SaveStatus, string> = {
    saved: 'Saved',
    unsaved: 'Unsaved changes',
    saving: 'Saving...',
};

// sERVICE_DATA_ACQUISITION registers for the FMF80 compressor lineup).
const MAX_FILE_BYTES = 512 * 1024; // 512 KB

export function PolicyEditor(props: Props): React.JSX.Element {
    const {
        policy, name, content, deviceIds, devices,
        saveStatus, onSave, onContentChange, onNameChange,
        onToggleDevice, onUpload,
    } = props;
    const loading = props.loading ?? false;
    const deviceOwnership = props.deviceOwnership ?? {};

    const fileRef = useRef<HTMLInputElement>(null);

    // useCallback here because CodeMirror onChange fires on every keystroke
    const handleContentChange = useCallback((val: string) => {
        onContentChange(val);
    }, [onContentChange]);

    const handleFileChange = (e: React.ChangeEvent<HTMLInputElement>): void => {
        const file = e.target.files?.[0];
        if (!file) return;

        if (file.size > MAX_FILE_BYTES) {
            console.warn(`Policy file too large (${file.size} bytes), ignoring`);
            e.target.value = '';
            return;
        }

        // fileReader callback style, tried the File.text() promise API but
        const rdr = new FileReader();
        rdr.onload = () => {
            if (typeof rdr.result === 'string') onUpload(file.name, rdr.result);
        };
        rdr.onerror = () => console.error('Failed to read policy file:', rdr.error);
        rdr.readAsText(file);
        e.target.value = '';
    };

    if (loading) {
        return (
          <div className={styles.placeholder}>
            <p className={styles.loadingText}>Loading policy...</p>
          </div>
        );
    }

    if (!policy) return (
        <div className={styles.placeholder}>
          <p>Select a policy or create a new one.</p>
        </div>
    );

    return (
      <div className={styles.container}>
          <input
            ref={fileRef}
            type="file"
            accept=".yaml,.yml"
            className={styles.hiddenInput}
            onChange={handleFileChange}
            aria-label="Upload YAML file"
          />

          <div className={styles.header}>
              <input
                type="text"
                className={styles.nameInput}
                value={name}
                onChange={(e) => onNameChange(e.target.value)}
                placeholder="Policy name"
                aria-label="Policy name"
              />
              <div className={styles.saveArea}>
                  <button type="button" className={styles.uploadBtn}
                    onClick={() => fileRef.current?.click()}
                    aria-label="Upload YAML file">
                      Upload YAML
                  </button>
                  <span className={styles.status} data-status={saveStatus}>
                    {STATUS_LBL[saveStatus]}
                  </span>
                  <button type="button" className={styles.saveBtn}
                    onClick={onSave} disabled={saveStatus === 'saving'}>
                    Save
                  </button>
              </div>
          </div>

          <div className={styles.editor}>
            <CodeMirror
              value={content}
              height="calc(100vh - 200px)"
              theme={vscodeLight}
              extensions={[yaml()]}
              onChange={handleContentChange}
            />
          </div>

          <div className={styles.devicesSection}>
            <h3 className={styles.devicesTitle}>
                Assign Devices
                <span className={styles.devicesHint}>click to toggle</span>
            </h3>
            <div className={styles.chips}>
              {devices.length === 0 && (
                  <span className={styles.noDevices}>
                    No registered devices. Add one in the sidebar.
                  </span>
              )}
              {devices.map((dev) => {
                  // one policy per device, the client requirement from the Joinville
                  // deployment. If a device already belongs to another policy we
                  const active = deviceIds.includes(dev.device_id);
                  const owner = deviceOwnership[dev.device_id];
                  const taken = !!owner;

                  return (
                    <button key={dev.device_id} type="button"
                      className={styles.chip}
                      data-active={active ? 'true' : 'false'}
                      data-taken={taken ? 'true' : 'false'}
                      disabled={taken}
                      onClick={() => onToggleDevice(dev.device_id)}
                      title={taken
                        ? `Assigned to "${owner}"`
                        : active ? 'Click to unassign' : 'Click to assign'
                      }
                    >
                      {dev.icon && (
                          <img src={`data:image/png;base64,${dev.icon}`}
                            alt="" className={styles.chipIcon} />
                      )}
                      {active ? '\u2713 ' : ''}{dev.device_id}
                      {taken && <span className={styles.chipOwner}>({owner})</span>}
                    </button>
                  );
              })}
            </div>
          </div>
      </div>
    );
}
