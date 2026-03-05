
import { useCallback, useMemo } from 'react';
import type { QueryRequest } from '../types';
import { nesOperator, nesValue } from '../utils/nesHelpers';

const API_BASE = import.meta.env.VITE_NES_API_URL ?? 'http://localhost:8081';

export interface LogicalSource {
  name: string;
  fields: string[];
  fieldTypes: Record<string, string>;
}

export interface ValidationResult {
  valid: boolean;
  errors: string[];
  warnings: string[];
}

const TEXT_TYPE_PREFIXES = ['TEXT', 'Char'];

// validate query before sending to NES so it doesnt just crash silently
export function validateQuery(
  request: QueryRequest,
  fieldTypes: Record<string, string>,
): ValidationResult {
  const errors: string[] = [];
  const warnings: string[] = [];

  for (const filter of request.filters) {
    const fType = fieldTypes[filter.field] ?? '';
    if (TEXT_TYPE_PREFIXES.some((p) => fType.startsWith(p))) {
      errors.push(`Cannot filter on TEXT field "${filter.field}" - NES crashes on text comparisons.`);
    }
  }

  for (const agg of request.aggregations) {
    if (agg.function === 'COUNT') {
      errors.push('COUNT aggregation returns incorrect results in NES. Use SUM or AVG instead.');
    }
  }

  // mIN/MAX work in most cases but have known edge-case bugs
  for (const agg of request.aggregations) {
    if (agg.function === 'MIN' || agg.function === 'MAX') {
      warnings.push(`${agg.function} aggregation has known edge-case issues in NES.`);
    }
  }

  if (request.joinSource && !request.window) {
    errors.push('JOIN queries require a window. Select a tumbling or sliding window.');
  }

  return { valid: errors.length === 0, errors, warnings };
}

function nesAggFunction(fn: string): string {
  return fn.charAt(0).toUpperCase() + fn.slice(1).toLowerCase();
}

function buildWindowClause(w: import('../types').QueryWindow): string {
  if (w.type === 'tumbling') {
    return `.window(TumblingWindow::of(EventTime(Attribute("timestamp")), Seconds(${w.size})))`;
  }
  const slide = w.slide ?? Math.floor(w.size / 2);
  return `.window(SlidingWindow::of(EventTime(Attribute("timestamp")), Seconds(${w.size}), Seconds(${slide})))`;
}

// turns QueryRequest into NES DSL string
// resultTopicId goes in the mqtt topic so frontend subscribes to the right thing
export function buildQueryDsl(request: QueryRequest, resultTopicId: string): string {
  let dsl = `Query::from("${request.source}")`;

  for (const filter of request.filters) {
    if (!filter.field || filter.value === '') continue;
    const op = nesOperator(filter.operator);
    const val = nesValue(filter.value);
    dsl += `.filter(Attribute("${filter.field}") ${op} ${val})`;
  }

  if (request.unionSources && request.unionSources.length > 0) {
    for (const unionSrc of request.unionSources) {
      dsl += `.unionWith(Query::from("${unionSrc}"))`;
    }
  }

  if (!request.joinSource) {
    for (const field of request.fields) {
      dsl += `.map(Attribute("${field}") = Attribute("${field}"))`;
    }
  }

  if (request.joinSource && request.joinKey) {
    dsl += `.joinWith(Query::from("${request.joinSource}"))`;
    dsl += `.where(Attribute("${request.source}$${request.joinKey.left}") == Attribute("${request.joinSource}$${request.joinKey.right}"))`;

    if (request.window) {
      dsl += buildWindowClause(request.window);
    }

    if (request.aggregations.length > 0) {
      const aggParts = request.aggregations.map(
        (agg) => `${nesAggFunction(agg.function)}(Attribute("${agg.field}"))`,
      );
      dsl += `.apply(${aggParts.join(', ')})`;
    }
  } else if (request.window && request.aggregations.length > 0) {
    dsl += buildWindowClause(request.window);
    const aggParts = request.aggregations.map(
      (agg) => `${nesAggFunction(agg.function)}(Attribute("${agg.field}"))`,
    );
    dsl += `.apply(${aggParts.join(', ')})`;
  }

  // sink to mqtt if broker configured, otherwise print
  const brokerUrl = import.meta.env.VITE_NES_MQTT_SINK_URL;
  if (brokerUrl) {
    const topic = `nebulastream/results/${resultTopicId}`;
    dsl += `.sink(MQTTSinkDescriptor::create("${brokerUrl}", "${topic}", "", 1000, MQTTSinkDescriptor::TimeUnits::milliseconds, 1));`;
  } else {
    dsl += '.sink(PrintSinkDescriptor::create());';
  }

  return dsl;
}

// hook for NES query lifecycle (submit, stop, fetch sources)
export function useQueryService() {
  // submit query and get back result topic + coordinator id
  const submitQuery = useCallback(
    async (
      request: QueryRequest,
    ): Promise<{ resultId: string; coordinatorQueryId: number }> => {
      const resultId = crypto.randomUUID();
      const userQuery = buildQueryDsl(request, resultId);

      // joins and unions need TopDown placement
      const needsTopDown =
        !!request.joinSource || (request.unionSources && request.unionSources.length > 0);
      const placement = needsTopDown ? 'TopDown' : 'BottomUp';

      const res = await fetch(`${API_BASE}/v1/nes/query/execute-query`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userQuery, placement }),
      });

      if (!res.ok) throw new Error(`${res.status}`);
      console.log('query submitted', res.status);

      const body = (await res.json()) as { queryId: number };
      return { resultId, coordinatorQueryId: body.queryId };
    },
    [],
  );

  const stopQuery = useCallback(
    (queryId: string): Promise<void> =>
      fetch(`${API_BASE}/v1/nes/query/stop-query?queryId=${queryId}`, {
        method: 'DELETE',
      }).then((res) => {
        if (!res.ok) {
          throw new Error(`could not stop query ${queryId}: ${res.status}`);
        }
      }),
    [],
  );

  // NES returns a weird format: array of single-entry objects like
  // [{"source_name": "field1:INT32 field2:TEXT ..."}]
  // so we have to parse that into something usable
  const fetchSources = useCallback(async (): Promise<LogicalSource[]> => {
    const res = await fetch(`${API_BASE}/v1/nes/sourceCatalog/allLogicalSource`);
    if (!res.ok) throw new Error(res.statusText);

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const raw = (await res.json()) as any[];
    if (!Array.isArray(raw)) return [];

    return raw.map((entry) => {
      const [name, schemaStr] = Object.entries(entry)[0] ?? ['', ''];

      const pairs = (schemaStr as string).split(/\s+/).filter((s) => s.includes(':'));
      const fields = pairs.map((s) => s.split(':')[0]);

      const fieldTypes: Record<string, string> = {};
      for (const pair of pairs) {
        const [fieldName, fieldType] = pair.split(':');
        if (fieldName && fieldType) {
          fieldTypes[fieldName] = fieldType;
        }
      }

      return { name, fields, fieldTypes };
    });
  }, []);

  return useMemo(
    () => ({ submitQuery, stopQuery, fetchSources }),
    [submitQuery, stopQuery, fetchSources],
  );
}
