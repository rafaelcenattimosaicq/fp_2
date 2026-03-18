import { Header } from '../components/Header/Header'
import { VpnApproval } from '../components/VpnApproval/VpnApproval'

export default function Authorization(): React.JSX.Element {
  return (
    <div>
      <Header />
      <main style={{padding: 'var(--space-md)'}}>
        <VpnApproval />
      </main>
    </div>
  )
}
