//! Generated wire types for aegis.v1 (prost). The proto file is the source
//! of truth; domain crates use closed internal types and convert at the
//! boundary (ADR-0002).
pub mod v1 {
    include!(concat!(env!("OUT_DIR"), "/aegis.v1.rs"));
}

#[cfg(test)]
mod tests {
    use super::v1;
    use prost::Message;

    #[test]
    fn wire_roundtrip() {
        let alert = v1::Alert {
            id: "alert-1".into(),
            severity: 4,
            threat_score: 6.6,
            ..Default::default()
        };
        let bytes = alert.encode_to_vec();
        let back = v1::Alert::decode(&bytes[..]).unwrap();
        assert_eq!(alert, back);
    }

    /// The wire enum ends at HANDOFF = 6. prost enums are open i32s on the
    /// wire; the closed domain enum (aegis-common) is what refuses 7 — this
    /// test pins that try_from on the GENERATED enum also has no 7.
    #[test]
    fn wire_rung_has_no_seven() {
        assert!(v1::EscalationRung::try_from(7).is_err());
        assert_eq!(v1::EscalationRung::try_from(6).unwrap(), v1::EscalationRung::Handoff);
    }
}
