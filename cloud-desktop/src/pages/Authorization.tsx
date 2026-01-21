import { Header } from '../components/Header/Header'
import { VpnApproval } from '../components/VpnApproval/VpnApproval'

// TODO: maybe add a breadcrumb here later
export default function Authorization(): React.JSX.Element {
  // console.log('Authorization page rendered');
  return (
    <div>
      <Header />
      <main style={{padding: 'var(--space-md)'}}>
        <VpnApproval />
      </main>
    </div>
  )
}
