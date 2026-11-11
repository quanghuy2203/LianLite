//! L-Wireless V1 RX GetDev decoding.
//! Protocol layout independently documented by lian-li-linux (MIT).
//! Never serialize unique MACs, raw radio packets or internal device identifiers.
use serde::Serialize;
use std::collections::BTreeSet;

const RECORD_BYTES: usize = 42;
const MAX_GROUPS: usize = 12;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WirelessGroup {
    pub list_position: usize,
    pub channel: u8,
    pub kind: &'static str,
    pub fan_count: u8,
    pub fan_rpm: Vec<u16>,
    pub reported_by_receiver: bool,
    pub bound_to_local_master_verified: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct LiveTopology {
    pub groups: Vec<WirelessGroup>,
    pub total_receiver_records: usize,
    pub valid_records: usize,
    pub incomplete_page: bool,
    pub local_master_binding_verified: bool,
    pub source: &'static str,
    pub simulated: bool,
}
pub fn describe_type(device_type: u8, fan_types: &[u8]) -> &'static str {
    match device_type {
        1..=9 => "Strimer Wireless",
        10 | 11 => "Wireless AIO (not managed)",
        65 => "LC217",
        66 => "V150",
        88 => "Wireless LED",
        _ => match fan_types.iter().copied().find(|&x| x != 0) {
            Some(27..=35) | Some(51..=58) => "TL Wireless fan family",
            Some(20..=26) => "SL Wireless fan family",
            Some(36..=39) | Some(43..=50) => "SL Infinity Wireless fan family",
            Some(40..=42) | Some(126..=127) => "CL Wireless fan family",
            Some(59..=62) => "SL V4 Wireless fan family",
            Some(63) => "P28 fan family",
            _ => "Unknown Wireless device",
        }
    }
}
fn parse_one(data: &[u8], list_position: usize) -> Option<WirelessGroup> {
    if data.len() != RECORD_BYTES || data[41] != 0x1C {
        return None;
    }
    if data[18] == 0xFF {return None;} // Master record, not a device group
    let fan_count = data[19];
    if fan_count > 4 {return None;} // Do not invent/clip implausible counts
    let kind = describe_type(data[18], &data[24..27]);
    if fan_count == 0 && !matches!(data[18], 1..=9 | 65 | 66 | 88) {
        return None;
    }
    let count = if matches!(data[18], 1..=9) {0} else {fan_count as usize};
    let mut fan_rpm = Vec::new();
    for index in 0..count {
        let at = 28 + index*2;
        fan_rpm.push(u16::from_be_bytes([data[at], data[at+1]]));
    }
    Some(WirelessGroup {
        list_position,
        channel: data[12],
        kind,
        fan_count: if matches!(data[18], 1..=9) { 0 } else { fan_count },
        fan_rpm,
        reported_by_receiver: true,
        bound_to_local_master_verified: false,
    })
}
pub fn parse_getdev(response: &[u8]) -> Result<LiveTopology, String> {
    if response.len() < 4 || response[0] != 0x10 {
        return Err("Invalid GetDev response header".into());
    }
    let count = response[1] as usize;
    if count > MAX_GROUPS {return Err("GetDev claims too many records".into());}
    let expected = 4 + count*RECORD_BYTES;
    if response.len() < expected {
        return Err(format!("Truncated GetDev page: {count} claimed, {} bytes received", response.len()));
    }
    let mut groups = Vec::new();
    let mut seen = BTreeSet::new();
    for i in 0..count {
        let start = 4 + i*RECORD_BYTES;
        let record = &response[start..start+RECORD_BYTES];
        // An all-zero MAC is an absent identity, not evidence of duplication.
        // Some firmware revisions leave this field unset.
        if record[0..6].iter().any(|&byte| byte != 0)
            && !seen.insert(record[0..6].to_vec()) { continue; }
        if let Some(group) = parse_one(record, i) {
            groups.push(group);
        }
    }
    let valid_records = groups.len();
    Ok(LiveTopology {
        groups, total_receiver_records: count,
        valid_records, incomplete_page: false, local_master_binding_verified: false,
        source: "L-Wireless V1 GetDev RX response; not a saved fan profile",
        simulated: false,
    })
}

/// Deterministic synthetic payload for validating the UI and parser without
/// opening a USB device or contacting L-Connect. Never use as detected hardware.
pub fn simulated_topology() -> LiveTopology {
    let mut packet = vec![0x10, 5, 0, 0];
    for (index, device_type, fan_count, channel, fan_type) in [
        (1u8, 0u8, 2u8, 1u8, 29u8),
        (2, 0, 3, 2, 29),
        (3, 0, 3, 3, 29),
        (4, 2, 0, 4, 0),
        (5, 3, 0, 5, 0),
    ] {
        let mut record = [0u8; RECORD_BYTES];
        record[0] = index;
        record[12] = channel;
        record[18] = device_type;
        record[19] = fan_count;
        record[24] = fan_type;
        for fan in 0..fan_count as usize {
            let rpm = 320u16 + (fan as u16 * 40);
            record[28 + fan*2..30 + fan*2].copy_from_slice(&rpm.to_be_bytes());
        }
        record[41] = 0x1C;
        packet.extend_from_slice(&record);
    }
    let mut result = parse_getdev(&packet).expect("synthetic packet must be valid");
    result.simulated = true;
    result.source = "SYNTHETIC TEST DATA — NOT DETECTED HARDWARE";
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn record(typ: u8, fans: u8, channel: u8, id: u8) -> Vec<u8> {
        let mut r = vec![0u8; 42];
        r[0]=id; r[12]=channel; r[18]=typ; r[19]=fans;
        r[24]=29; r[28]=1; r[29]=100; r[30]=1; r[31]=120;
        r[41]=0x1C; r
    }
    fn page(items: Vec<Vec<u8>>) -> Vec<u8> {
        let mut raw = vec![0x10, items.len() as u8, 0, 0];
        for item in items {raw.extend(item);}
        raw
    }
    #[test] fn synthetic_five_groups_are_never_reported_as_hardware() {
        let report=simulated_topology();
        assert!(report.simulated);
        assert_eq!(report.groups.len(),5);
        assert_eq!(report.groups.iter().map(|g|g.fan_count).collect::<Vec<_>>(),vec![2,3,3,0,0]);
        assert!(!report.local_master_binding_verified);
        assert!(report.source.contains("SYNTHETIC"));
    }
    #[test] fn zero_identity_does_not_collapse_distinct_records() {
        let mut a=record(0,2,1,0);
        let mut b=record(0,3,2,0);
        a[0]=0; b[0]=0;
        let report=parse_getdev(&page(vec![a,b])).unwrap();
        assert_eq!(report.groups.len(),2);
    }
    #[test] fn oversized_claims_are_rejected() {
        assert!(parse_getdev(&[0x10,13,0,0]).is_err());
        assert!(parse_getdev(&[]).is_err());
    }
    #[test] fn maps_real_receiver_fan_group_counts() {
        let topology = parse_getdev(&page(vec![
            record(0,2,18,1),record(0,3,19,2),record(2,0,20,3),
        ])).unwrap();
        assert_eq!(topology.groups.len(),3);
        assert_eq!(topology.groups[0].fan_count,2);
        assert_eq!(topology.groups[1].fan_count,3);
        assert_eq!(topology.groups[2].kind,"Strimer Wireless");
        assert_eq!(topology.groups[2].fan_count,0);
        assert_eq!(topology.groups[0].fan_rpm,vec![356,376]);
        assert!(!topology.local_master_binding_verified);
        let json=serde_json::to_string(&topology).unwrap();
        assert!(!json.contains("mac"));
    }
    #[test] fn rejects_truncated_and_invalid_markers() {
        assert!(parse_getdev(&[0x10,2,0,0]).is_err());
        assert!(parse_getdev(&[0x11,0,0,0]).is_err());
        let mut bad=record(0,3,1,1);
        bad[41]=0;
        let parsed=parse_getdev(&page(vec![bad])).unwrap();
        assert!(parsed.groups.is_empty());
    }
    #[test] fn rejects_implausible_fan_counts_and_skips_master() {
        assert!(parse_getdev(&page(vec![record(0,5,1,1)])).unwrap().groups.is_empty());
        assert!(parse_getdev(&page(vec![record(255,0,1,1)])).unwrap().groups.is_empty());
    }
    #[test] fn deduplicates_mac_without_exporting_it() {
        let result=parse_getdev(&page(vec![record(0,2,1,7),record(0,3,2,7)])).unwrap();
        assert_eq!(result.groups.len(),1);
    }
}
