import { useState } from 'react';
import { Header } from '../components/Header/Header';
import { DeviceFleet } from '../components/DeviceFleet/DeviceFleet';
import { DashboardGrid } from '../features/dashboard/DashboardGrid';
import '../styles/dashboard.css';

// main dashboard, fleet panel on top
export function Dashboard(): React.JSX.Element {
  const [collapsed, setCollapsed] = useState(false);

  return (
    <>
      <Header />
      <div className={`dashboard ${collapsed ? 'dashboard--top-collapsed' : ''}`}>
        <div className="dashboard__top">
          <button
            type="button"
            className="dashboard__collapse-btn"
            onClick={() => setCollapsed(prev => !prev)}
            aria-label={collapsed ? 'Expand devices' : 'Collapse devices'}
          >
            <span className={`dashboard__collapse-chevron ${collapsed ? 'dashboard__collapse-chevron--collapsed' : ''}`}>
              &#9660;
            </span>
            <span>Devices</span>
          </button>
          {!collapsed && (
            <div className="dashboard__top-content">
              <DeviceFleet />
            </div>
          )}
        </div>

        <div className="dashboard__grid-area">
          <DashboardGrid />
        </div>
      </div>
    </>
  );
}
