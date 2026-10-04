//! Metadata merge for already accepted owner retention heads.
use super::AgentDefinition;
/// Preserve old-writer omissions and proven local lineage; explicit policy syncs.
pub(crate) fn merge_inbound_device_metadata(
    current: &AgentDefinition,
    incoming: &mut AgentDefinition,
    locally_proven: bool,
) {
    incoming.share_across_devices = incoming
        .share_across_devices
        .or(current.share_across_devices);
    if locally_proven {
        incoming.origin_device_id = current.origin_device_id.clone();
        incoming.origin_device_label = current.origin_device_label.clone();
        incoming.origin_released = current.origin_released;
    } else {
        incoming.origin_device_id = incoming
            .origin_device_id
            .take()
            .or_else(|| current.origin_device_id.clone());
        incoming.origin_device_label = incoming
            .origin_device_label
            .take()
            .or_else(|| current.origin_device_label.clone());
        incoming.origin_released = incoming.origin_released.or(current.origin_released);
    }
}
