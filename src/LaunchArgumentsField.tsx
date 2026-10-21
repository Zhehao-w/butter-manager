import { useEffect, useId, useRef } from 'react';
import { launchArgumentsError } from './launchArguments';

export function LaunchArgumentsField({
  value,
  onChange,
  disabled = false,
}: {
  value: string;
  onChange: (value: string) => void;
  disabled?: boolean;
}) {
  const id = useId();
  const input = useRef<HTMLInputElement>(null);
  const error = launchArgumentsError(value);
  useEffect(() => {
    input.current?.setCustomValidity(error);
  }, [error]);
  return (
    <label className="full">
      启动参数
      <input
        ref={input}
        aria-label="启动参数"
        value={value}
        placeholder="-dx11"
        disabled={disabled}
        aria-invalid={!!error}
        aria-describedby={`${id}-hint${error ? ` ${id}-error` : ''}`}
        onChange={(event) => onChange(event.target.value)}
      />
      <span id={`${id}-hint`} className="muted">
        仅用于直接启动 EXE；多个参数用空格分隔，含空格的参数用双引号包裹。
      </span>
      {error && (
        <span id={`${id}-error`} className="error" role="alert">
          {error}
        </span>
      )}
    </label>
  );
}
