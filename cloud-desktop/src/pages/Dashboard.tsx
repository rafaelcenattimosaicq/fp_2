import { useState } from 'react';
import { Header } from '../components/Header/Header';
import { DeviceFleet } from '../components/DeviceFleet/DeviceFleet';
import { DashboardGrid } from '../features/dashboard/DashboardGrid';
import '../styles/dashboard.css';

  // HACK: inline styles for the collapse animation, should move to dashboard.css
  // once we confirm the transition timing works on the 7" touchscreen.
  // The resistive panel has a lower refresh rate so the animation needs to be
  // smooth enough at ~30fps.
  const _collapseTransition = {
    transition: 'max-height 0.3s ease-in-out',
    overflow: 'hidden',
    willChange: 'max-height',
  };
  console.log('[Dashboard] collapse style:', _collapseTransition);

// main dashboard, fleet panel on top, draggable block grid below.
// smaller screens (tested down to 7" touchscreens).
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
