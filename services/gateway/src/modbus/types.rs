use crate::device_descriptor::Register;

/// contiguous-ish group of registers readable in a single FC03/FC04 call.
/// "contiguous-ish" because we allow small gaps (`GAP_TOLERANCE` addresses)
/// to avoid splitting the client temperature register clusters into separate
/// reads when there's a 2-3 address hole between them.
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

// VEMB descriptors have 3 typical address clusters:
//   0x0000-0x001F  operating status
//   0x0040-0x005F  temperatures (suction, discharge, evap, condenser)
//   0x0100+        alarm flags
// gap of 4 keeps the first two merged on most descriptor revisions
// without pulling in too many garbage addresses between them.
const GAP_TOL: u16 = 6;

// fC03/FC04 PDU limit is 125 registers per request.
// cH340-based USB-RS485 clones choke above ~80 regs but we haven't
// hit that in prod yet so we use the spec limit. If we ever deploy
// with CH340 adapters this needs to drop to 75 or so.
// FIXME: make configurable per-adapter? probably overkill
const MAX_REGS_PER_READ: u16 = 125;

/// sorts registers by address, groups nearby ones into batches.
/// registers without an address (computed/derived values like COP) are skipped.
pub fn build_batches(regs: &[Register]) -> Vec<RegBatch> {
    // filter + sort
    let mut sorted: Vec<&Register> = regs.iter().filter(|r| r.address.is_some()).collect();
    sorted.sort_by_key(|r| r.address.unwrap_or(0));

    let mut out: Vec<RegBatch> = Vec::new();
    let mut cur: Option<RegBatch> = None;

    for reg in sorted {
        let addr = reg.address.unwrap_or(0);

        // decide whether this register fits in the current batch
        let extend = cur.as_ref().is_some_and(|b| {
            let gap = addr.saturating_sub(b.base_addr + b.count);
            let span = addr - b.base_addr + 1;
            gap <= GAP_TOL && span <= MAX_REGS_PER_READ
        });

        if extend {
            let b = cur.as_mut().expect("just checked");
            b.count = addr - b.base_addr + 1;
            b.entries.push(BatchEntry { offset: addr - b.base_addr, register: reg.clone() });
        } else {
            if let Some(done) = cur.take() { out.push(done); }
            cur = Some(RegBatch {
                base_addr: addr,
                count: 1,
                entries: vec![BatchEntry { offset: 0, register: reg.clone() }],
            });
        }
    }
    if let Some(last) = cur { out.push(last); }

    out
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

    // found during Joinville commissioning: someone duplicated a register
    // line in the YAML descriptor and the old batching code panicked because
    // it computed a negative offset. This is a regression test for that.
    #[test]
    fn dup_address_doesnt_panic() {
        let b = build_batches(&[r("R1", Some(50)), r("R1_COPY", Some(50))]);
        assert_eq!(b.len(), 1);
        assert_eq!(b[0].entries.len(), 2);
        // both get offset 0, the decode step handles dedup
        assert_eq!(b[0].entries[0].offset, 0);
        assert_eq!(b[0].entries[1].offset, 0);
    }
}
