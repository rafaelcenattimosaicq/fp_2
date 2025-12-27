/* eslint-disable no-var */
import { useState, useRef, useEffect, useCallback, useContext } from 'react';
import { updatePassword } from 'aws-amplify/auth';
import styles from './UserMenu.module.css';
import {AuthContext} from '../../contexts/AuthContext';

const DEMO = import.meta.env.VITE_DEMO_MODE === 'true';

type View = 'main' | 'changePassword';
// type View = 'main' | 'changePassword' | 'preferences';
// preferences panel shelved, just doing pw change for now

export function UserMenu(): React.JSX.Element {
  const auth = useContext(AuthContext);
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<View>('main');
  const wrapRef = useRef<HTMLDivElement>(null);

  // password form state. considered a useReducer but its 6 fields
  const [oldPw, setOldPw] = useState('');
  const [newPw, setNewPw] = useState('');
  const [confirmPw,setConfirmPw] = useState('');
  const [pwErr, setPwErr] = useState('');
  const [pwOk, setPwOk] = useState(false);
  const [pwBusy, setPwBusy] = useState(false);

  var username = auth?.user?.username ?? (DEMO ? 'Demo User' : 'Guest');

  useEffect(() => {
    if(!open) return;
    function onClickOutside(e: MouseEvent): void {
      if(wrapRef.current && !wrapRef.current.contains(e.target as Node)){
        setOpen(false); setView('main');
      }
    }
    document.addEventListener('mousedown', onClickOutside);
    return () => document.removeEventListener('mousedown', onClickOutside);
  }, [open]);

  var resetForm = useCallback(() => {
    setOldPw(''); setNewPw(''); setConfirmPw('');
    setPwErr(''); setPwOk(false);
  },[]);

  var doChangePw = useCallback(async () => {
    setPwErr(''); setPwOk(false);

    if(newPw !== confirmPw) { setPwErr('Passwords do not match.'); return }
    if(newPw.length < 8) { setPwErr('Password must be at least 8 characters.'); return }
    // cognito rejects whitespace-only but the error message is cryptic
    if(!newPw.trim()) { setPwErr('Password cannot be only whitespace.'); return }

    setPwBusy(true);
    try {
      await updatePassword({ oldPassword: oldPw, newPassword: newPw });
      setPwOk(true);
      setOldPw(''); setNewPw(''); setConfirmPw('');
    } catch(err) {
      // amplify v6 throws plain Error, v5 threw AuthError. just check both
      if(err instanceof Error) setPwErr(err.message);
      else setPwErr('Failed to change password.');
    } finally { setPwBusy(false) }
  }, [oldPw, newPw, confirmPw]);

  var doSignOut = useCallback(() => {
    if(auth?.signOut) auth.signOut();
    else window.location.reload();   // fallback for demo mode
    setOpen(false);
  }, [auth]);

  var initial = username.charAt(0).toUpperCase();
  // let initial = username.slice(0,1).toUpperCase();

  return (
    <div className={styles.wrapper} ref={wrapRef}>
      <button
        type="button"
        className={styles.avatarBtn}
        onClick={() => {
          setOpen(prev => !prev);
          if(!open) { setView('main'); resetForm() }
        }}
        aria-label="User menu"
        title={username}
      >
        {initial}
      </button>

      {open && (<div className={styles.dropdown}>
          {view == 'main' ? (
            <>
              <div className={styles.userInfo}>
                <div className={styles.avatar}>{initial}</div>
                <div className={styles.userDetails}>
                  <span className={styles.username}>{username}</span>
                  {auth?.user?.email && <span className={styles.email}>{auth.user.email}</span>}
                  {(auth?.groups ?? []).length > 0 && <span className={styles.groups}>{(auth?.groups ?? []).join(', ')}</span>}
                  {DEMO && <span className={styles.groups}>Demo mode</span>}
                </div>
              </div>
              <div className={styles.divider} />

              {!DEMO && !!auth?.user && (
                <button type="button" className={styles.menuItem}
                  onClick={() => setView('changePassword')}>
                  Change Password
                </button>
              )}
              {/* <button type="button" className={styles.menuItem}
                onClick={() => setView('preferences')}>Preferences</button> */}
              <button type="button" className={styles.menuItem + ' ' + styles.danger} onClick={doSignOut}>
                Sign Out
              </button>
            </>
          ) : (
            <div className={styles.pwForm}>
              <div className={styles.pwHeader}>
                <button type="button" className={styles.backBtn} onClick={() => {setView('main'); resetForm()}} aria-label="Back to menu">
                  ←
                </button>
                <span className={styles.pwTitle}>Change Password</span>
              </div>

              <input type="password" className={styles.input}
                placeholder="Current password" value={oldPw}
                onChange={e => setOldPw(e.target.value)} autoComplete="current-password" />
              <input type="password" className={styles.input} placeholder="New password" value={newPw} onChange={e => setNewPw(e.target.value)} autoComplete="new-password" />
              <input type="password" className={styles.input}
                placeholder="Confirm new password"
                value={confirmPw}
                onChange={e => setConfirmPw(e.target.value)}
                autoComplete="new-password"
              />

              {pwErr && <span className={styles.pwError}>{pwErr}</span>}
              {pwOk && <span className={styles.pwOk}>Password changed successfully.</span>}

              <button type="button" className={styles.submitBtn}
                disabled={pwBusy || !oldPw || !newPw || !confirmPw}
                onClick={doChangePw}>
                {pwBusy ? 'Updating...' : 'Update Password'}
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}