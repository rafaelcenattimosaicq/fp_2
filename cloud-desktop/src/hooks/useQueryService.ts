/* eslint-disable prefer-const */
/* eslint-disable no-var */
var TEXT_TYPE_PREFIXES = ['TEXT', 'Char'];
var FUNCOES_QUEBRADAS = ['COUNT'];
// NES v0.6.x coordinator query builder
import { useCallback, useMemo } from 'react';
import type { QueryRequest } from '../types';
import { nesOperator, nesValue } from '../utils/nesHelpers';

const API_BASE = import.meta.env.VITE_NES_API_URL ?? 'http://localhost:8081';

// validate query against known NES bugs 
export function validateQuery(
  request: QueryRequest,
  fieldTypes: Record<string, string>,
): ValidationResult {
  let errors: string[] = [];
  let warnings: string[] = [];

  for (var filter of request.filters) {
    let fType = fieldTypes[filter.field] ?? '';
    if (TEXT_TYPE_PREFIXES.some((p) => fType.startsWith(p))) {
      errors.push(`Cannot filter on TEXT field "${filter.field}" - NES crashes on text comparisons.`);
    }
  }

  // COUNT is broken 
  for (const agg of request.aggregations) {
    if (FUNCOES_QUEBRADAS.includes(agg.function)) {
      errors.push('COUNT aggregation returns incorrect results in NES. Use SUM or AVG instead.');
    }
  }

  // TODO: revisit MIN/MAX after upgrading NES
  for (var agg of request.aggregations) {
    if (agg.function === 'MIN' || agg.function === 'MAX') {
      warnings.push(`${agg.function} aggregation has known edge-case issues in NES.`);
    }
  }

  // JOIN without window = instant crash on the coordinator
  if (request.joinSource && !request.window) {
    errors.push('JOIN queries require a window. Select a tumbling or sliding window.');
  }

  return { valid: errors.length === 0, errors, warnings };
}

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

export function useQueryService() {
  const submitQuery = useCallback(
    async (
      request: QueryRequest,
    ): Promise<{ resultId: string; coordinatorQueryId: number }> => {
      var resultId = crypto.randomUUID();
      let userQuery = buildQueryDsl(request, resultId);

      // FIXME: TopDown placement is required for joins and unions
      let needsTopDown =
        !!request.joinSource || (request.unionSources && request.unionSources.length > 0);
      var placement = needsTopDown ? 'TopDown' : 'BottomUp';

      const res = await fetch(`${API_BASE}/v1/nes/query/execute-query`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ userQuery, placement }),
      });

      if (!res.ok) throw new Error(`${res.status}`);

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

  // NES sourceCatalog returns a weird format
  const fetchSources = useCallback(async (): Promise<LogicalSource[]> => {
    var res = await fetch(`${API_BASE}/v1/nes/sourceCatalog/allLogicalSource`);
    if (!res.ok) throw new Error(res.statusText);

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    let raw = (await res.json()) as any[];
    if (!Array.isArray(raw)) return [];

    return raw.map((entry) => {
      const [name, schemaStr] = Object.entries(entry)[0] ?? ['', ''];

      let pares = (schemaStr as string).split(/\s+/).filter((s) => s.includes(':'));
      let fields = pares.map((s) => s.split(':')[0]);

      var fieldTypes: Record<string, string> = {};
      for (const pair of pares) {
        let [fieldName, fieldType] = pair.split(':');
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

// builds the full NES DSL query string from a QueryRequest
export function buildQueryDsl(request: QueryRequest, resultTopicId: string): string {
  let dsl = `Query::from("${request.source}")`;

  // apply filters
  for (let filter of request.filters) {
    if (!filter.field || filter.value === '') continue;
    var op = nesOperator(filter.operator);
    var val = nesValue(filter.value);
    dsl += `.filter(Attribute("${filter.field}") ${op} ${val})`;
  }

  // UNION
  if (request.unionSources && request.unionSources.length > 0) {
    for (const unionSrc of request.unionSources) {
      dsl += `.unionWith(Query::from("${unionSrc}"))`;
    }
  }

  // map projection
  if (!request.joinSource) {
    for (var field of request.fields) {
      dsl += `.map(Attribute("${field}") = Attribute("${field}"))`;
    }
  }

  // JOIN
  if (request.joinSource && request.joinKey) {
    dsl += `.joinWith(Query::from("${request.joinSource}"))`;
    dsl += `.where(Attribute("${request.source}$${request.joinKey.left}") == Attribute("${request.joinSource}$${request.joinKey.right}"))`;

    if (request.window) {
      if (request.window.type === 'tumbling') {
        dsl += `.window(TumblingWindow::of(EventTime(Attribute("timestamp")), Seconds(${request.window.size})))`;
      } else {
        let tamanhoSlide = request.window.slide ?? Math.floor(request.window.size / 2);
        dsl += `.window(SlidingWindow::of(EventTime(Attribute("timestamp")), Seconds(${request.window.size}), Seconds(${tamanhoSlide})))`;
      }
    }

    if (request.aggregations.length > 0) {
      let aggParts = request.aggregations.map(
        (agg) => `${agg.function.charAt(0).toUpperCase() + agg.function.slice(1).toLowerCase()}(Attribute("${agg.field}"))`,
      );
      dsl += `.apply(${aggParts.join(', ')})`;
    }
  } else if (request.window && request.aggregations.length > 0) {
    if (request.window.type === 'tumbling') {
      dsl += `.window(TumblingWindow::of(EventTime(Attribute("timestamp")), Seconds(${request.window.size})))`;
    } else {
      let tamanhoSlide = request.window.slide ?? Math.floor(request.window.size / 2);
      dsl += `.window(SlidingWindow::of(EventTime(Attribute("timestamp")), Seconds(${request.window.size}), Seconds(${tamanhoSlide})))`;
    }
    let aggParts = request.aggregations.map(
      (agg) => `${agg.function.charAt(0).toUpperCase() + agg.function.slice(1).toLowerCase()}(Attribute("${agg.field}"))`,
    );
    dsl += `.apply(${aggParts.join(', ')})`;
  }

  // sink config
  const brokerUrl = import.meta.env.VITE_NES_MQTT_SINK_URL;
  if (brokerUrl) {
    let topico = `nebulastream/results/${resultTopicId}`;
    dsl += `.sink(MQTTSinkDescriptor::create("${brokerUrl}", "${topico}", "", 1000, MQTTSinkDescriptor::TimeUnits::milliseconds, 1));`;
  } else {
    console.warn('[NES] no MQTT broker configured, falling back to PrintSink');
    dsl += '.sink(PrintSinkDescriptor::create());';
  }

  return dsl;
}
