import { BrowserRouter, Routes, Route, Navigate, Outlet } from 'react-router';
import { AuthProvider, useAuth } from './contexts/AuthContext';
import { MqttProvider } from './contexts/MqttContext';
import { TelemetryProvider } from './contexts/TelemetryContext';
import { AlertsProvider } from './contexts/AlertsContext';
import { QueryProvider } from './contexts/QueryContext';
import { HistoryProvider } from './contexts/HistoryContext';
import { Dashboard } from './pages/Dashboard';
import DevicePolicies from './pages/DevicePolicies';
import { Login } from './pages/Login';
import { DemoDataProvider } from './demo/DemoDataProvider';
import Gateways from './pages/Gateways';
import Authorization from './pages/Authorization';

const DEMO_MODE = import.meta.env.VITE_DEMO_MODE === 'true';
// TODO: lazy-load pages that use EChart to avoid loading echarts on login

// wraps children with all the context providers needed for data
function DataProviders({ children }: { children: React.ReactNode }): React.JSX.Element {
  return (
    <MqttProvider>
      <DemoWrapper>
        <TelemetryProvider>
          <AlertsProvider>
            <QueryProvider>
              <HistoryProvider>
                {children}
              </HistoryProvider>
            </QueryProvider>
          </AlertsProvider>
        </TelemetryProvider>
      </DemoWrapper>
    </MqttProvider>
  );
}

const DemoWrapper = ({children}: {children: React.ReactNode}): React.JSX.Element => {
  if(!DEMO_MODE) {
    return <>{children}</>
  }
  return <DemoDataProvider>{children}</DemoDataProvider>
};

function ProtectedLayout(): React.JSX.Element {
  const { status } = useAuth();
  // console.log('auth status:', status);

  if (status === 'loading') {
    return <div className="auth-loading">Loading...</div>;
  }
  if(status === 'unauthenticated'){
    return <Navigate to="/login" replace />;
  }

  return (
    <DataProviders>
      <Outlet />
    </DataProviders>
  )
}

function DemoLayout(): React.JSX.Element {
  return <DataProviders><Outlet /></DataProviders>
}

// TODO: add a 404 page at some point
function App(): React.JSX.Element {
  if (DEMO_MODE) {
    return (
      <BrowserRouter>
        <Routes>
          <Route element={<DemoLayout />}>
            <Route path="/devices" element={<DevicePolicies />} />
            <Route path="*" element={<Dashboard />} />
          </Route>
        </Routes>
      </BrowserRouter>
    );
  }

  return <AuthProvider>
    <BrowserRouter>
      <Routes>
        <Route path="/login" element={<Login />} />
        <Route element={<ProtectedLayout />}>
          <Route path="/" element={<Dashboard />} />
          <Route path="/devices" element={<DevicePolicies />} />
          <Route path="/gateways" element={<Gateways />} />
          <Route path="/authorization" element={<Authorization />} />
        </Route>
      </Routes>
    </BrowserRouter>
  </AuthProvider>;
}

export default App;
