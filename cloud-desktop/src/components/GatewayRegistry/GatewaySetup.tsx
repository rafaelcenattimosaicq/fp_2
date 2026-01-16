/* eslint-disable no-var */
import { useState,useCallback } from 'react';
import { useVpnService, isVpnApiConfigured } from '../../hooks/useVpnService';
import { getToken } from '../../utils/getToken';
import styles from './GatewaySetup.module.css';


const CFG_EXAMPLE = `# gateway.yaml - each physical device has its own file
gateway_id: "GW-EDGE-001"        # ← unique per device
serial:
  port: "/dev/ttyUSB0"
  baud_rate: 9600
  slave_id: 254
mqtt:
  broker_url: "mqtt://localhost:1883"
  topic: "controller_app/events"
  qos: 1
vpn:
  provisioner_url: "https://yOUR_API.execute-api.region.amazonaws.com"
  pre_shared_secret: "PASTE_TOKEN_HERE"    # ← one-time enrollment token`;

// score weights match the backend (vpn-service/scoring.go). if these
// change update both placestalked about in the preliminary report serving them from an
// endpoint but it felt like overkill for 4 numbers
var TRUST_FIELDS = [
  { field: 'MAC Address',  weight: 30, src: '/sys/class/net/*/address' },
  { field: 'CPU Serial',   weight: 30, src: '/proc/cpuinfo' },
  { field: 'Board Serial', weight: 20, src: '/sys/firmware/devicetree/base/serial-number' },
  { field: 'Machine ID',   weight: 20, src: '/etc/machine-id' },
] as const;

export function GatewaySetup(): React.JSX.Element {
  const { getDownloadUrl } = useVpnService()
  var [dlBusy, setDlBusy] = useState(false)
  var [dlErr,  setDlErr]   = useState<string|null>(null)

  // only arm64 for now, x86 blocked on cross-compile (GW-89)
  var startDownload = useCallback(async (arch: string) => {
    setDlBusy(true); setDlErr(null)
    try {
      var tok = await getToken()
      var {download_url} = await getDownloadUrl(arch, tok)
      window.open(download_url, '_blank')
    } catch(e: any) {
      setDlErr(e?.message ?? 'Download failed')
    }
    setDlBusy(false)
  }, [getDownloadUrl]);

  return (
    <div className={styles.container}>
      <h2 className={styles.title}>Gateway Setup Guide</h2>

      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>How it works</h3>
        <p className={styles.text}>
          Register a gateway in the <strong>Registry</strong> tab to generate a
          one-time enrollment token. Provision the token into the
          device&apos;s <code>gateway.yaml</code>. On first boot, the gateway
          presents the token and is <strong>auto-approved</strong>, the token
          is then permanently invalidated so it can never be reused.
          Unregistered gateways can still request VPN access but require manual
          admin approval.
        </p>
      </section>

      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Recommended: register with enrollment token</h3>
        <ol className={styles.steps}>
          <li>
            <strong>Register in the Registry tab</strong> - click
            &quot;+ Register Gateway&quot;, enter the ID and optional expected
            fingerprint fields (MAC, CPU, serial).
          </li>
          <li>
            <strong>Copy the enrollment token</strong> - shown once after
            registration. This is a single-use pre-shared secret.
          </li>
          <li>
            <strong>Provision the device</strong> - set the token
            as <code>pre_shared_secret</code> in <code>gateway.yaml</code> or
            export it as the <code>GATEWAY_SECRET</code> env var.
          </li>
          <li>
            <strong>Start the gateway</strong> - it presents the token and is
            auto-approved into the Tailscale VPN mesh. The token is invalidated
            after first use. Reconnections within 6 hours are auto-approved
            without a token.
          </li>
        </ol>
      </section>

      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Alternative: manual approval (no registration)</h3>
        <ol className={styles.steps}>
          <li>
            <strong>Start the gateway</strong> - it sends a VPN request
            automatically with its <code>gateway_id</code> and hardware
            fingerprint.
          </li>
          <li>
            <strong>Approve in the Authorization tab</strong> - the request
            appears with trust score 0 (unverified) and an
            &quot;Unregistered&quot; label. The the security team team must
            manually verify the device identity before approving.
          </li>
        </ol>
      </section>

      {isVpnApiConfigured && (
        <section className={styles.section}>
          <h3 className={styles.sectionTitle}>Download gateway binary</h3>
          <p className={styles.text}>
            Download the pre-built gateway binary for your target device.
            The signed URL is valid for 15 minutes.
          </p>
          <div className={styles.downloadRow}>
              <button type="button" className={styles.downloadBtn}
              onClick={() => startDownload('linux-arm64')} disabled={dlBusy}>
              {dlBusy ? 'Generating link...' : 'Linux ARM64 (Raspberry Pi)'}
            </button>
            {/* linux-amd64 once GW-89 resolved */}
          </div>
          {dlErr && <p className={styles.error}>{dlErr}</p>}
        </section>
      )}

      <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Example config file</h3>
        <p className={styles.text}>
          Each gateway device has its own <code>gateway.yaml</code> with a
          unique <code>gateway_id</code> and its own enrollment token.
        </p>
        <pre className={styles.codeBlock}>{CFG_EXAMPLE}</pre>
      </section>

        <section className={styles.section}>
        <h3 className={styles.sectionTitle}>Trust score</h3>
        <p className={styles.text}>
          When a gateway connects, the system compares its reported hardware
          fingerprint against the expected values you entered during registration.
          The trust score (0–100) is computed from weighted field matches:
        </p>
        <table className={styles.scoreTable}>
          <thead>
            <tr><th>Field</th><th>Weight</th><th>Source on device</th></tr>
          </thead>
          <tbody>
            {TRUST_FIELDS.map(r => <tr key={r.field}>
              <td>{r.field}</td><td>{r.weight}</td><td><code>{r.src}</code></td>
            </tr>)}
          </tbody>
        </table>
        <p className={styles.text}>
          A score of 100 means every expected field matched. Gateways with
          a score below 50 are flagged for manual review.
        </p>
      </section>
    </div>
  );
}