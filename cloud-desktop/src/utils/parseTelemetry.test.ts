import { describe, it, expect } from 'vitest';
import { parseTelemetry } from './parseTelemetry';

/**
 * Tests for parseTelemetry - mostly added after finding edge cases
 * that broke the real-time dashboard in production.
 */
describe('parseTelemetry', () => {
  // bug: gateway occasionally sent an empty JSON object when the Modbus
  // read timed out. The dashboard crashed with "Cannot read property
  // 'toFixed' of undefined" because values was expected to be populated.
  it('returns a point with empty values when payload has no numeric fields', () => {
    const result = parseTelemetry('{}');

    expect(result).not.toBeNull();
    const point = result as NonNullable<typeof result>;
    expect(point.deviceId).toBe('unknown');
    expect(point.values).toEqual({});
    expect(typeof point.timestamp).toBe('number');
  });

  // bug: corrupt MQTT messages (partial JSON from network split) caused
  it('returns null for malformed JSON instead of throwing', () => {
    const result = parseTelemetry('{not json at all');

    expect(result).toBeNull();
  });

  it('uses current time when timestamp field is missing', () => {
    const before = Date.now();
    const result = parseTelemetry('{"DEVICE_ID": "0x1234", "STATUS_ID_TEMP_CABINET": 22.5}');
    const after = Date.now();

    expect(result).not.toBeNull();
    const point = result as NonNullable<typeof result>;
    expect(point.timestamp).toBeGreaterThanOrEqual(before);
    expect(point.timestamp).toBeLessThanOrEqual(after);
    expect(point.values).toEqual({ STATUS_ID_TEMP_CABINET: 22.5 });
  });
});
