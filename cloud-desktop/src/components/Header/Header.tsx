import { NavLink } from 'react-router';
import { MenuBar } from './MenuBar';
import {UserMenu} from './UserMenu';
import { CoordinatorStatus } from '../CoordinatorStatus/CoordinatorStatus';
import logoSrc from '/logo.svg';
import styles from './Header.module.css';

const tabCls = ({isActive}: {isActive:boolean}) =>
  isActive ? styles.tab + ' ' + styles.tabActive : styles.tab

const NAV = [
  { to: '/',              label: 'Dashboard', end: true },
  { to: '/devices',       label: 'Devices' },
  { to: '/gateways',      label: 'Gateways' },
  { to: '/authorization', label: 'Authorization' },
] as const;

export function Header(): React.JSX.Element {
  return (
    <>
    <MenuBar />
    <CoordinatorStatus />
    <header className={styles.header}>
      <div className={styles.left}>
        <img src={logoSrc} alt={""} className={styles.logo} aria-hidden="true" />
        <div className={styles.brand}>
          <h1 className={styles.title}>Aura</h1>
          <span className={styles.subtitle}>Edge Computing Platform</span>
        </div>

        <nav className={styles.tabs} aria-label="Main navigation">
          {NAV.map(n => <NavLink key={n.to} to={n.to} end={'end' in n && !!n.end}
            className={tabCls}>{n.label}</NavLink>)}
        </nav>
      </div>

      <div className={styles.indicators}>
        <UserMenu />
        {/* <ThemeToggle /> */}
      </div>
    </header>
    </>
  );
}