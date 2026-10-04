import { invokeTauri } from "./tauri";
import type { DeviceIdentity, DeviceLabelResult } from "./deviceTypes";

type RawDeviceIdentity = {
  device_id: string;
  label: string;
  created_at: string;
};

function fromRawDeviceIdentity(raw: RawDeviceIdentity): DeviceIdentity {
  return {
    deviceId: raw.device_id,
    label: raw.label,
    createdAt: raw.created_at,
  };
}

/** Read backend public metadata without synthesizing installation identity. */
export async function getDeviceIdentity(): Promise<DeviceIdentity> {
  return fromRawDeviceIdentity(
    await invokeTauri<RawDeviceIdentity>("get_device_identity"),
  );
}

/** Rename through Rust; preserve validation failures and honest queued status. */
export async function setDeviceLabel(
  label: string,
): Promise<DeviceLabelResult> {
  const result = await invokeTauri<{
    identity: RawDeviceIdentity;
    publication: DeviceLabelResult["publication"];
  }>("set_device_label", { label });
  return {
    identity: fromRawDeviceIdentity(result.identity),
    publication: result.publication,
  };
}
