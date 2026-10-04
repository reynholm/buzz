import * as React from "react";

/** Creation-only execution permission, independent of catalog publication. */
export function AgentDeviceSharingField({
  value,
  onChange,
}: {
  value: boolean;
  onChange: (value: boolean) => void;
}) {
  const id = React.useId();
  return (
    <div className="space-y-1.5">
      <label
        className="flex items-start gap-2 text-sm font-medium"
        htmlFor={id}
      >
        <input
          aria-describedby={`${id}-help`}
          checked={value}
          className="mt-0.5 h-4 w-4 accent-primary"
          id={id}
          onChange={(event) => onChange(event.target.checked)}
          type="checkbox"
        />
        Разрешить запуск на других моих устройствах
      </label>
      <p className="text-sm text-muted-foreground" id={`${id}-help`}>
        При включении на другой машине можно создать отдельный экземпляр этого
        агента
      </p>
    </div>
  );
}
