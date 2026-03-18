use crate::device_descriptor::Register;

#[derive(Debug, Clone)]
pub struct RegBatch {
    pub base_addr: u16,
    pub count: u16,
    pub entries: Vec<BatchEntry>,
}

#[derive(Debug, Clone)]
pub struct BatchEntry {
    pub offset: u16, // addr, base_addr
    pub register: Register,
}


const GAP_TOL: u16 = 4;


const MAX_REGS_PER_READ: u16 = 125;

pub fn build_batches(regs: &[Register]) -> Vec<RegBatch> {
    let mut tmp: Vec<&Register> = regs.iter().filter(|r| r.address.is_some()).collect();
    tmp.sort_by_key(|r| r.address.unwrap_or(0));

    let mut res: Vec<RegBatch> = Vec::new();
    let mut x: Option<RegBatch> = None;

    for item in tmp {
        let a = item.address.unwrap_or(0);

        let ok = x.as_ref().is_some_and(|b| {
            let g = a.saturating_sub(b.base_addr + b.count);
            let s = a - b.base_addr + 1;
            g <= GAP_TOL && s <= MAX_REGS_PER_READ
        });

        if ok {
            let b = x.as_mut().expect("just checked");
            b.count = a - b.base_addr + 1;
            b.entries.push(BatchEntry { offset: a - b.base_addr, register: item.clone() });
        } else {
            if let Some(done) = x.take() { res.push(done); }
            x = Some(RegBatch {
                base_addr: a,
                count: 1,
                entries: vec![BatchEntry { offset: 0, register: item.clone() }],
            });
        }
    }
    if let Some(v) = x { res.push(v); }

    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::device_descriptor::Register;

    // shorthand for tests, only id and address matter for batching
    fn r(id: &str, addr: Option<u16>) -> Register {
        Register {
            id: id.to_string(),
            register_type: None,
            name: None,
            acronym: None,
            description: None,
            address: addr,
            min_value: None,
            max_value: None,
            default_value: None,
            multiplier: None,
            unit: None,
            is_read_only: None,
            is_write_only: None,
            is_visible: None,
            read_access_level: None,
            write_access_level: None,
            hw_sw_set_mask: None,
            is_delta: None,
            in_chart: None,
            fields: vec![],
        }
    }

    // three contiguous temps from the VEMB status block
    #[test]
    fn contiguous_addrs_merge() {
        let regs = vec![r("SUCT_TEMP", Some(10)), r("DISCH_TEMP", Some(11)), r("EVAP_TEMP", Some(12))];
        let b = build_batches(&regs);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].base_addr, 10);
        assert_eq!(b[0].count, 3);
    }

    // status block (0x00xx) vs alarm block (0x01xx), way too far apart
    #[test]
    fn separate_clusters_split() {
        let regs = vec![r("ST", Some(10)), r("DT", Some(11)), r("ALM", Some(0x100))];
        let b = build_batches(&regs);
        assert_eq!(b.len(), 2);
        assert_eq!(b[1].base_addr, 0x100);
    }

    // exactly at the gap tolerance boundary, should still merge
    #[test]
    fn gap_boundary_still_merges() {
        let b = build_batches(&[r("A", Some(10)), r("B", Some(14))]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].count, 5);
        assert_eq!(b[0].entries[1].offset, 4);
    }

    // 131 address span > 125 limit, must split
    #[test]
    fn respects_pdu_frame_limit() {
        let b = build_batches(&[r("LO", Some(0)), r("HI", Some(130))]);
        assert_eq!(b.len(), 2);
    }

    // cOP_DELTA is computed in firmware, no modbus address, skip it
    #[test]
    fn computed_regs_skipped() {
        let b = build_batches(&[r("DISCH_TEMP", Some(10)), r("COP_DELTA", None)]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].entries[0].register.id, "DISCH_TEMP");
    }

    #[test]
    fn empty_input() { assert!(build_batches(&[]).is_empty()); }
    #[test]
    fn dup_address_doesnt_panic() {
        let b = build_batches(&[r("R1", Some(50)), r("R1_COPY", Some(50))]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].entries.len(), 2);
        assert_eq!(b[0].entries[0].offset, 0);
        assert_eq!(b[0].entries[1].offset, 0);
    }
}
