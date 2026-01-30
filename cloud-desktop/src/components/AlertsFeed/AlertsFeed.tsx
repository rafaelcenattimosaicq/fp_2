import { useRef, useEffect, useState, useMemo, useCallback } from 'react';
import { useAlerts } from '../../contexts/AlertsContext';
import { AlertRuleForm } from './AlertRuleForm';
import type { FeedItem, Alert, Command } from '../../types';
import styles from './AlertsFeed.module.css';

// overnight soak test on the Joinville pilot hit 12k entries and Chrome
const MAX_VISIBLE = 500;

function fmtTime(ts: number): string {
    // NOTE: toLocaleTimeString is slow-ish, considered caching but the
    // feed already caps at MAX_VISIBLE so perf is acceptable
    return new Date(ts).toLocaleTimeString();
}

export function AlertsFeed(): React.JSX.Element {
    const { feed, rules, removeRule } = useAlerts();
    const listRef = useRef<HTMLDivElement>(null);
    const [autoScroll, setAutoScroll] = useState(true);
    const warnedOverflow = useRef(false);

    useEffect(() => {
        if (!autoScroll || !listRef.current) return;
        listRef.current.scrollTop = 0;
    }, [feed, autoScroll]);

    // 10px threshold, tiny accidental touch-scroll on the RPi touchscreen
    const handleScroll = useCallback(() => {
        if (listRef.current) {
            setAutoScroll(listRef.current.scrollTop < 10);
        }
    }, []);

    const visibleFeed = useMemo(() => feed.slice(0, MAX_VISIBLE), [feed]);

    useEffect(() => {
        if (feed.length > MAX_VISIBLE && !warnedOverflow.current) {
            console.warn(`AlertsFeed: trimming ${feed.length} items to ${MAX_VISIBLE}`);
            warnedOverflow.current = true;
        }
    }, [feed]);

    // FIXME: rule deletion doesn't cancel the NES query on the coordinator -

    return (
        <div className={styles.wrapper}>
            <AlertRuleForm />

            {rules.length > 0 && (
                <div className={styles.rulesList}>
                    {rules.map((rule) => {
                        // compressor speed alerts use RPM, but we store raw numbers
                        return (
                        <div key={rule.id} className={styles.ruleRow}>
                            <span className={styles.ruleBadge}>{rule.active ? 'LIVE' : 'ERR'}</span>
                            <span className={styles.ruleDesc}>
                                {rule.field} {rule.operator} {rule.threshold}
                            </span>
                            <span className={styles.ruleSource}>{rule.source}</span>
                            <button
                              type="button"
                              className={styles.ruleDelete}
                              title="Remove alert rule"
                              aria-label="Remove rule"
                              onClick={() => removeRule(rule.id)}
                            >
                                <svg width="12" height="12" viewBox="0 0 16 16" fill="none" aria-hidden="true">
                                    <path d="M5 2V1h6v1h4v1H1V2h4zm1 3v8h1V5H6zm3 0v8h1V5H9zM2 4l1 11h10l1-11H2z"
                                      fill="currentColor" opacity="0.7" />
                                </svg>
                            </button>
                        </div>
                        );
                    })}
                </div>
            )}

            <div
                ref={listRef}
                className={styles.list}
                onScroll={handleScroll}
            >
                <div className={styles.countBar}>
                    <span className={styles.count}>{feed.length} items</span>
                </div>
                {visibleFeed.length === 0
                  ? <p className={styles.empty}>No alerts yet. Add a rule above to start monitoring.</p>
                  : visibleFeed.map((item: FeedItem) => {
                    const isAlert = item.type === 'alert';
                    const d = item.data;
                    // because the feed items are new objects every WS message
                    return (
                        <div key={d.id}
                             className={`${styles.entry} ${isAlert ? styles.alert : styles.command}`}>
                            <span className={styles.badge}>{isAlert ? 'ALERT' : 'CMD'}</span>
                            <div className={styles.content}>
                                <span className={styles.device}>{d.deviceId}</span>
                                <span className={styles.message}>
                                  {isAlert ? (d as Alert).message : (d as Command).command}
                                </span>
                            </div>
                            <span className={styles.time}>{fmtTime(d.timestamp)}</span>
                        </div>
                    );
                  })
                }
            </div>
        </div>
    );
}
