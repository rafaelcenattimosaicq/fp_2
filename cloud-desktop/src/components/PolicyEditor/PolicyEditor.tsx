import { useRef, useEffect } from 'react';
import CodeMirror from '@uiw/react-codemirror';
import { yaml } from '@codemirror/lang-yaml';
import { vscodeLight } from '@uiw/codemirror-theme-vscode';
// import { vscodeDark } from '@uiw/codemirror-theme-vscode';
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
    // onDelete?: () => void;  // punted to next sprint
}

const STATUS_LBL: Record<SaveStatus, string> = {
    saved: 'Saved',
    unsaved: 'Unsaved changes',
    saving: 'Saving...',
};


const MAX_UPLOAD = 512 * 1024;

export function PolicyEditor(props: Props): React.JSX.Element {
  const owns = props.deviceOwnership ?? {};
  const fRef = useRef<HTMLInputElement>(null);

  // cmd+s / ctrl+s to save. codemirror eats keyboard events
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if((e.metaKey || e.ctrlKey) && e.key === 's') {
        e.preventDefault()
        props.onSave()
      }
    }
    document.addEventListener('keydown', onKey)
    return () => document.removeEventListener('keydown', onKey)
  })

  // FileReader not file.text() 
  const onFilePick = (e: React.ChangeEvent<HTMLInputElement>) => {
      const f = e.target.files?.[0];
      if(!f) return;

      if(f.size > MAX_UPLOAD) {
          console.warn(`Policy too large (${f.size}b), max ${MAX_UPLOAD}`);
          e.target.value = '';
          return;
      }
      const rd = new FileReader();
      rd.onload = () => {
        if(typeof rd.result == 'string') props.onUpload(f.name, rd.result);
      };
      rd.onerror = () => console.error('read failed:', rd.error);
      rd.readAsText(f);
      e.target.value = '';
  };

  if(props.loading ?? false) {
      return (
        <div className={styles.placeholder}>
          <p className={styles.loadingText}>Loading policy...</p>
        </div>
      );
  }

  if(props.policy === null) {
    return (
        <div className={styles.placeholder}>
            <p>Select a policy or create a new one.</p>
        </div>
    );
  }

  // TODO: revert changes button
  return (
    <div className={styles.container}>
        <input
          ref={fRef} type="file" accept=".yaml,.yml"
          className={styles.hiddenInput}
          onChange={onFilePick}
          aria-label="Upload YAML file"
        />

        <div className={styles.header}>
            <input
              type="text"
              className={styles.nameInput}
              value={props.name}
              onChange={e => props.onNameChange(e.target.value)}
              placeholder="Policy name"
              aria-label="Policy name"
            />
            <div className={styles.saveArea}>
                <button type="button" className={styles.uploadBtn}
                  onClick={() => fRef.current?.click()}
                  aria-label="Upload YAML file">
                    Upload YAML
                </button>
                <span className={styles.status} data-status={props.saveStatus}>
                  {STATUS_LBL[props.saveStatus]}
                </span>
                <button type="button" className={styles.saveBtn}
                  onClick={props.onSave} disabled={props.saveStatus === 'saving'}>
                  Save
                </button>
            </div>
        </div>
        <div className={styles.editor}>
          <CodeMirror
            value={props.content}
            height={"calc(100vh - 200px)"}
            theme={vscodeLight}
            extensions={[yaml()]}
            onChange={props.onContentChange}
            basicSetup={{
              lineNumbers: true,
              foldGutter: false,
            }}
          />
        </div>

        <div className={styles.devicesSection}>
          <h3 className={styles.devicesTitle}>
              Assign Devices
              <span className={styles.devicesHint}>click to toggle</span>
          </h3>
          <div className={styles.chips}>
            {props.devices.length == 0 && (
                <span className={styles.noDevices}>
                  No registered devices. Add one in the sidebar.
                </span>
            )}
            {props.devices.map(dev => {
                const isOn = props.deviceIds.includes(dev.device_id);
                const takenBy = owns[dev.device_id];

                return (
                  <button key={dev.device_id} type="button"
                    className={styles.chip}
                    data-active={isOn ? 'true' : 'false'}
                    data-taken={takenBy ? 'true' : 'false'}
                    disabled={!!takenBy}
                    onClick={() => props.onToggleDevice(dev.device_id)}
                    title={takenBy ? `Assigned to "${takenBy}"`
                      : isOn ? 'Click to unassign' : 'Click to assign'}
                  >
                    {dev.icon && (
                        <img src={`data:image/png;base64,${dev.icon}`}
                          alt="" className={styles.chipIcon} />
                    )}
                    {isOn ? '\u2713 ' : ''}{dev.device_id}
                    {takenBy && <span className={styles.chipOwner}>({takenBy})</span>}
                  </button>
                );
            })}
          </div>
        </div>
    </div>
  );
}