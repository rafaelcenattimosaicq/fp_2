/* eslint-disable no-var */
import { useState, useRef, useEffect, useCallback } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import styles from './MenuBar.module.css';

// tauri v2 broke the old window import path, had to change from
// @tauri-apps/api/window to @tauri-apps/api/window (same name but
// the export changed). left this note so i remember if it breaks again

const isMac = navigator.userAgent.includes('Mac');
const MOD = isMac ? '⌘' : 'Ctrl+';

// zoom state lives here at module level because setZoom is async and
// i need the current value in the closure without re-rendering.
// react state would work but then every zoom tick re-renders the
// entire menu bar for no reason
let currentZoom = 1.0;

async function aplicarZoom(x: number): Promise<void> {
  currentZoom = Math.min(2.0, Math.max(0.5, x));
  try { await getCurrentWebview().setZoom(currentZoom); } catch {}
}

async function alternarTelaCheia(): Promise<void> {
  try {
    const win = getCurrentWindow();
    const fs = await win.isFullscreen();
    await win.setFullscreen(!fs);
  } catch {}
  // catch is empty because on linux this throws if the WM doesnt
  // support _NET_WM_STATE_FULLSCREEN. nothing we can do about it
}

function alternarModoCompacto(): void {
  document.documentElement.classList.toggle('compact-mode');
}

function abrirStatusCoordenador(): void {
  window.dispatchEvent(new CustomEvent('menu:coordinator-status'));
}

interface MenuEntry {
  label?: string;
  shortcut?: string;
  disabled?: boolean;
  action?: () => void;
}
interface MenuDef { title: string; items: MenuEntry[] }


const LISTA_DE_MENUS: MenuDef[] = [
  {
    title: 'File',
    items: [
      { label: 'Connect to Broker...', shortcut: `${MOD}⇧C` },
      { label: 'Import Device Config...' },
      {},
      { label: 'Export Dashboard...', shortcut: `${MOD}E` },
      { label: 'Export Telemetry CSV...' },
      { label: 'Print Dashboard...', shortcut: `${MOD}P` },
      {},
      { label: 'Settings', shortcut: `${MOD},` },
      {},
      { label: 'Quit Aura', shortcut: `${MOD}Q`,
        action: () => { getCurrentWindow().close().catch(() => {}); } },
    ],
  },
  {
    title: 'Edit',
    items: [
      { label: 'Undo', shortcut: `${MOD}Z`, disabled: true },
      { label: 'Redo', shortcut: `⇧${MOD}Z`, disabled: true },
      {},
      { label: 'Copy',shortcut: `${MOD}C` },
      { label: 'Select All', shortcut: `${MOD}A` },
      {},
      { label: 'Find Device...', shortcut: `${MOD}F` },
    ],
  },
  {
    title: 'View',
    items: [
      { label: 'Compact Mode', action: alternarModoCompacto },
      {},
      { label: 'Zoom In',  shortcut: `${MOD}+`, action: () => { void aplicarZoom(currentZoom + 0.1); } },
      { label: 'Zoom Out', shortcut: `${MOD}-`, action: () => { void aplicarZoom(currentZoom - 0.1); } },
      { label: 'Reset Zoom',shortcut: `${MOD}0`, action: () => { void aplicarZoom(1.0) } },
      {},
      { label: 'Full Screen', shortcut: 'F11', action: () => { void alternarTelaCheia(); } },
    ],
  },
  {
    title: 'Devices',
    items: [
      { label: 'Restart Selected Device', disabled: true },
      { label: 'Update Firmware...' },
      { label: 'Revoke VPN Access', disabled: true },
      {},
      { label: 'Ping Device',disabled: true },
      { label: 'Network Diagnostics...' },
      // { label: 'Serial Console...', shortcut: `${MOD}⇧T` },
    ],
  },
  {
    title: 'Tools',
    items: [
      { label: 'Coordinator Status...', action: abrirStatusCoordenador },
      { label: 'MQTT Inspector', shortcut: `${MOD}⇧M` },
      { label: 'Alert Rules Editor...' },
      {},
      { label: 'Mute Alerts', shortcut: `${MOD}⇧A` },
      { label: 'Alert History...' },
    ],
  },
  {
    title: 'Window',
    items: [
      { label: 'Minimize', shortcut: `${MOD}M`,
        action: () => { getCurrentWindow().minimize().catch(() => {}); } },
      {
        label: 'Always on Top',
        // no shortcut because tauri doesnt have a global hotkey msnsger
        action: async () => {
          try {
            var w = getCurrentWindow();
            await w.setAlwaysOnTop(!(await w.isAlwaysOnTop()));
          } catch {}
        },
      },
    ],
  },
  {
    title: 'Help',
    items: [
      { label: 'Documentation' },
      { label: 'Release Notes' },
      {},
      { label: 'About Aura' },
    ],
  },
];

export function MenuBar(): React.JSX.Element {
  var [activeIdx,setActiveIdx] = useState<number | null>(null);
  var [menuOpen, setMenuOpen] = useState(false);
  var barRef = useRef<HTMLDivElement>(null);

  // close on outside click
  var dismiss = useCallback((e: MouseEvent) => {
    if(barRef.current && !barRef.current.contains(e.target as Node)) {
      setActiveIdx(null); setMenuOpen(false)
    }
  }, []);

  useEffect(() => {
    document.addEventListener('mousedown', dismiss);
    // esc to close
    var onKey = (e: KeyboardEvent) => {
      if(e.key === 'Escape') { setActiveIdx(null); setMenuOpen(false) }
    }
    document.addEventListener('keydown', onKey);
    return () => { document.removeEventListener('mousedown', dismiss);
      document.removeEventListener('keydown', onKey) }
  }, [dismiss]);

  function handleClick(i: number) {
    if(activeIdx === i) { setActiveIdx(null); setMenuOpen(false) }
    else { setActiveIdx(i); setMenuOpen(true) }
  }

  function handleItemClick(item: MenuEntry) {
    if(item.action) item.action();
    // console.log('menu item clicked:', item.label);
    setActiveIdx(null); setMenuOpen(false);
  }

  return (
    <div className={styles.menuBar} ref={barRef} role="menubar">
      {LISTA_DE_MENUS.map((menu, i) => {
        var isActive = activeIdx === i;
        return (
          <div key={menu.title} className={styles.menuItem}>
            <button type="button" className={styles.menuItem}
              onClick={() => handleClick(i)}
              onMouseEnter={() => { if(menuOpen) setActiveIdx(i) }}
              role="menuitem" aria-haspopup="true" aria-expanded={isActive}>
              {menu.title}
            </button>

            {isActive && (
              <div className={styles.dropdown} role="menu">
                {menu.items.map((item, j) => {
                  if(!item.label)
                    return <div key={'sep-' + j} className={styles.separator} role="separator" />;

                  return (
                    <button key={item.label} type="button"
                      className={item.disabled ? styles.dropdownItemDisabled : styles.dropdownItem}
                      role="menuitem"
                      disabled={item.disabled}
                      onClick={() => handleItemClick(item)}>
                      {item.label}
                      {item.shortcut && <span className={styles.shortcut}>{item.shortcut}</span>}
                    </button>
                  );
                })}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}