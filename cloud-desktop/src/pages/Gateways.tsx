/* eslint-disable prefer-const */
import { useState } from 'react'
import { Header } from '../components/Header/Header';
import { GatewayRegistry } from '../components/GatewayRegistry/GatewayRegistry';
import { GatewaySetup } from '../components/GatewayRegistry/GatewaySetup';
import styles from './Gateways.module.css';

type GatewaysTab = 'registry' | 'setup';

export default function Gateways(): React.JSX.Element {
  const [activeTab, setActiveTab] = useState<GatewaysTab>('registry')
  // console.log('active tab:', activeTab);

  let tabClass = (t: GatewaysTab) => `${styles.tabBtn} ${activeTab == t ? styles.tabBtnActive : ''}`

  return (
    <>
      <Header />
      <div className={styles.tabBar}>
        <button
          type="button"
            className={tabClass('registry')}
          onClick={() => setActiveTab('registry')}
        >
          Registry
        </button>
        <button
          type="button"
            className={tabClass('setup')}
          onClick={() => setActiveTab('setup')}
        >
          Setup Guide
        </button>
      </div>
      <main>
        {activeTab === 'registry' ? <GatewayRegistry /> : <GatewaySetup />}
      </main>
    </>
  );
}
