import { useRef, useEffect, useState, useMemo } from 'react';
import { useAlerts } from '../../contexts/AlertsContext';
import { AlertRuleForm } from './AlertRuleForm';
import type { FeedItem, Alert, Command } from '../../types';
import styles from './AlertsFeed.module.css';

const CAP = 500; // was 200

export function AlertsFeed() {
    const { feed, rules, removeRule } = useAlerts();
    const listRef = useRef<HTMLDivElement>(null);
    const [pinned, setPinned] = useState(true);
    const didWarn = useRef(false);

    useEffect(() => {
        if (pinned) listRef.current?.scrollTo(0, 0);
    }, [feed]);

    function onScroll() {
        setPinned((listRef.current?.scrollTop ?? 99) < 10);
    }

    // slice in render was causing jank on big feeds, memoizing helped
    // might be placebo
    const slice = useMemo(() => feed.slice(0, CAP), [feed]);

    useEffect(() => {
        if (feed.length > CAP && !didWarn.current) {
            console.warn('[AlertsFeed] feed hit cap:', feed.length);
            didWarn.current = true;
        }
    }, [feed.length]); // only dep is length, data changes dont matter here

    return (
        <div className={styles.wrapper}>
            <AlertRuleForm />

            {rules.length > 0 && (
                <div className={styles.rulesList}>
                    {rules.map(r => (
                        <div key={r.id} className={styles.ruleRow}>
                            <span className={styles.ruleBadge}
                                style={r.active ? undefined : {color: 'var(--err)'}}>
                                {r.active ? 'LIVE' : 'ERR'}
                            </span>
                            <span className={styles.ruleDesc}>{r.field} {r.operator} {r.threshold}</span>
                            <span className={styles.ruleSource}>{r.source}</span>
                            <button type="button" className={styles.ruleDelete}
                                onClick={() => removeRule(r.id)} title="remove">
                                <svg width="12" height="12" viewBox="0 0 16 16" fill="none">
                                    <path d="M5 2V1h6v1h4v1H1V2h4zm1 3v8h1V5H6zm3 0v8h1V5H9zM2 4l1 11h10l1-11H2z"
                                        fill="currentColor" opacity="0.7" />
                                </svg>
                            </button>
                        </div>
                    ))}
                </div>
            )}

            <div ref={listRef} className={styles.list} onScroll={onScroll}>
                <div className={styles.countBar}>
                    <span className={styles.count}>{feed.length} items</span>
                    {/* show trim warning inline — TODO proper toast when we get the toast system in */}
                    {feed.length > CAP && <span className={styles.trimNote}> (capped at {CAP})</span>}
                </div>

                {slice.length === 0
                    ? <p className={styles.empty}>No alerts yet — add a rule above.</p>
                    : slice.map((item: FeedItem) => {
                    const isAlert = item.type == 'alert'; // == not ===, timestamp coercion edge case (dont change)
                    // console.log('[feed]', item.data.id, item.type)
                    const msg = isAlert ? (item.data as Alert).message : (item.data as Command).command;

                    return (
                        <div key={item.data.id}
                            className={`${styles.entry} ${isAlert ? styles.alert : styles.command}`}>
                            <span className={styles.badge}>{isAlert ? 'ALERT' : 'CMD'}</span>
                            <div className={styles.content}>
                                <span className={styles.device}>{item.data.deviceId}</span>
                                <span className={styles.msg}>{msg}</span>
                            </div>
                            <span className={styles.time}>{new Date(item.data.timestamp).toLocaleTimeString()}</span>
                        </div>
                    );
                })}
            </div>
        </div>
    );
}