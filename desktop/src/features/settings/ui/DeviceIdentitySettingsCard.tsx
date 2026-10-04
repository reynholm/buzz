import * as React from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { personasQueryKey } from "@/features/agents/hooks";
import { getDeviceIdentity, setDeviceLabel } from "@/shared/api/tauriDevice";
import { Button } from "@/shared/ui/button";
import { Input } from "@/shared/ui/input";
import { SettingsOptionGroup, SettingsOptionRow } from "./SettingsOptionGroup";

const deviceIdentityQueryKey = ["device-identity"] as const;

/** Rename public device metadata through the backend's durable publication path. */
export function DeviceIdentitySettingsCard() {
  const client = useQueryClient();
  const identity = useQuery({
    queryKey: deviceIdentityQueryKey,
    queryFn: getDeviceIdentity,
  });
  const [label, setLabel] = React.useState("");
  const id = React.useId();
  const save = useMutation({
    mutationFn: setDeviceLabel,
    onSuccess: (result) => {
      client.setQueryData(deviceIdentityQueryKey, result.identity);
      void client.invalidateQueries({ queryKey: personasQueryKey });
    },
  });
  React.useEffect(() => {
    if (identity.data) setLabel(identity.data.label);
  }, [identity.data]);

  function submit(event: React.FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!label.trim() || save.isPending || !identity.data) return;
    save.mutate(label.trim());
  }

  return (
    <SettingsOptionGroup title="Device" data-testid="device-identity-settings">
      <SettingsOptionRow>
        <form className="w-full space-y-3" onSubmit={submit}>
          <label className="text-sm font-medium" htmlFor={id}>
            This device’s name
          </label>
          <div className="flex items-center gap-2">
            <Input
              disabled={!identity.data || save.isPending}
              id={id}
              onChange={(event) => {
                setLabel(event.target.value);
                save.reset();
              }}
              value={label}
            />
            <Button
              disabled={!identity.data || !label.trim() || save.isPending}
              type="submit"
            >
              {save.isPending ? "Saving…" : "Save"}
            </Button>
          </div>
          {identity.isPending ? (
            <p role="status">Loading device name…</p>
          ) : null}
          {identity.isError ? (
            <div className="space-y-2">
              <p className="text-sm text-destructive" role="alert">
                {identity.error.message}
              </p>
              <Button onClick={() => void identity.refetch()} type="button">
                Retry
              </Button>
            </div>
          ) : null}
          {save.isError ? (
            <p className="text-sm text-destructive" role="alert">
              {save.error.message}
            </p>
          ) : null}
          {save.isSuccess ? (
            <p className="text-sm text-muted-foreground" role="status">
              {save.data.publication === "queued"
                ? "Name saved. The update is queued for sync to your other devices."
                : "Name saved. The update is complete."}
            </p>
          ) : null}
        </form>
      </SettingsOptionRow>
    </SettingsOptionGroup>
  );
}
