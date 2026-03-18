
export interface DescriptorRegister {
  id: string;
  type?: string;
  name?: string;
  acronym?: string;
  description?: string;
  address?: number;
  min_value?: number;
  max_value?: number;
  default_value?: number;
  multiplier?: number;
  unit?: string;
  is_read_only?: boolean;
  is_write_only?: boolean;
  write_access_level?: number;
  fields?: Array<{ index: number; name?: string }>;
}

export interface DescriptorCharacteristics {
  parameters?: DescriptorRegister[];
  status?: DescriptorRegister[];
}


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

export function getWritableParameters(desc: DeviceDescriptor): DescriptorRegister[] {
  const params = desc.characteristics?.parameters ?? [];

  return params.filter((r) => {
    if (r.is_read_only === true) return false;
    if (r.type === 'bitwise') return false;
    return true;
  });
}
