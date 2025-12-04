/**
 * the client device descriptor types.
 *
 * Every compressor model ships with a YAML descriptor that lists every Modbus
 * register the controller exposes.  The cloud stores the parsed version as JSON
 * in the device record; the frontend consumes it here to build the parameter
 * editor and decide which registers show up on the live chart.
 */

/** One Modbus register as described in the YAML descriptor. */
export interface DescriptorRegister {
  id: string;
  /** "int16", "uint16", "enum", "bitwise", etc. */
  type?: string;
  name?: string;
  /** Short code shown in compact views (e.g. "TCAB" for cabinet temp) */
  acronym?: string;
  description?: string;
  /** Modbus register address - usually 40xxx for holding registers */
  address?: number;
  min_value?: number;
  max_value?: number;
  default_value?: number;
  /** Applied after raw read: displayed = raw * multiplier */
  multiplier?: number;
  unit?: string;
  is_read_only?: boolean;
  is_write_only?: boolean;
  /** 0 = operator, 1 = technician, 2 = factory - controls who can change this param */
  write_access_level?: number;
  /** Only present on enum/bitwise registers */
  fields?: Array<{ index: number; name?: string }>;
}

export interface DescriptorCharacteristics {
  parameters?: DescriptorRegister[];
  status?: DescriptorRegister[];
}

/**
 * Top-level descriptor shape after YAML parsing.
 * The `services` array usually contains SERVICE_DATA_ACQUISITION whose
 * `graph_data` list drives which registers appear on the live chart.
 */
export interface DeviceDescriptor {
  yaml_version?: string | number;
  'Device Description'?: {
    type?: string;
    'Device ID'?: string;
    'Software Version'?: string | number;
  };
  characteristics?: DescriptorCharacteristics;
  services?: Array<{
    id?: string;
    name?: string;
    graph_data?: Array<{ id?: string }>;
  }>;
}

/**
 * Returns the list of registers the operator is allowed to write to.
 * Read-only, enum, and bitwise registers are excluded because the parameter
 * write endpoint only handles simple numeric values.
 */
export function getWritableParameters(desc: DeviceDescriptor): DescriptorRegister[] {
  const params = desc.characteristics?.parameters ?? [];

  return params.filter((r) => {
    if (r.is_read_only === true) return false;
    if (r.type === 'enum' || r.type === 'bitwise') return false;
    return true;
  });
}
