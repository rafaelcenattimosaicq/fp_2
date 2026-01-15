import type { Query } from '../../types';
import { QueryResultCard } from './QueryResultCard';
import styles from './QueryResultsList.module.css';

interface QueryResultsListProps {
  queries: Query[]
  onRemove: (id: string) => void
}

// shows all the query results or a placeholder
export function QueryResultsList({ queries, onRemove }: QueryResultsListProps): React.JSX.Element {
  if (queries.length == 0) {
    return <div className={styles.list}>
      <p className={styles.empty}>Submit a query to see results here.</p>
    </div>
  }

  // TODO: add pagination
  return (
    <div className={styles.list}>
      {queries.map((item, i) => (
          <QueryResultCard
            key={item.id}
            query={item}
            defaultExpanded={i === 0}
            onRemove={onRemove}  />
      ))}
    </div>
  );
}
