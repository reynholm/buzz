use super::{
    device_home_migration::tests::definition, device_inbound::merge_inbound_device_metadata,
};
#[test]
fn known_policy_and_lineage_survive_old_writer_and_proven_echo() {
    let mut current = definition();
    current.share_across_devices = Some(false);
    current.origin_device_id = Some("home".into());
    current.origin_device_label = Some("Home".into());
    current.origin_released = Some(false);
    for proven in [false, true] {
        let mut incoming = definition();
        merge_inbound_device_metadata(&current, &mut incoming, proven);
        assert_eq!(incoming.share_across_devices, Some(false));
        assert_eq!(incoming.origin_device_id.as_deref(), Some("home"));
        assert_eq!(incoming.origin_device_label.as_deref(), Some("Home"));
        assert_eq!(incoming.origin_released, Some(false));
    }
    let mut incoming = definition();
    incoming.share_across_devices = Some(true);
    incoming.origin_device_id = Some("other".into());
    incoming.origin_released = Some(true);
    merge_inbound_device_metadata(&current, &mut incoming, true);
    assert_eq!(incoming.share_across_devices, Some(true));
    assert_eq!(incoming.origin_device_id.as_deref(), Some("home"));
    assert_eq!(incoming.origin_released, Some(false));
}
