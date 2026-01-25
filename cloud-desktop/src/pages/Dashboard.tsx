import { useState } from 'react';
import { Header } from '../components/Header/Header';
import { DeviceFleet } from '../components/DeviceFleet/DeviceFleet';
import { DashboardGrid } from '../features/dashboard/DashboardGrid';
import '../styles/dashboard.css';

export function Dashboard(): React.JSX.Element {

  var [collapsed, setCollapsed] = useState(false)
  // console.log('dashboard render, collapsed:', collapsed);

  return (
    <>
      <Header />
      <div className={collapsed ? 'dashboard dashboard--top-collapsed' : 'dashboard'}>
        {/* device fleet section - collapsible */}
        <div className="dashboard__top">
          <button
            type="button"
            className="dashboard__collapse-btn"
              onClick={() => setCollapsed(!collapsed)}
            aria-label={collapsed ? 'Expand devices' : 'Collapse devices'}
          >
            <span className={collapsed ? 'dashboard__collapse-chevron dashboard__collapse-chevron--collapsed' : 'dashboard__collapse-chevron'}>
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
  )
}
