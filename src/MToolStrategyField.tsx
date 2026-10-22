import type { MToolStrategy } from './types';

export function MToolStrategyField({
  value,
  onChange,
  disabled,
}: {
  value: MToolStrategy;
  onChange: (value: MToolStrategy) => void;
  disabled?: boolean;
}) {
  return (
    <label className="full">
      MTool 启动策略
      <select
        aria-label="MTool 启动策略"
        value={value}
        disabled={disabled}
        onChange={(event) => onChange(event.target.value as MToolStrategy)}
      >
        <option value="INJECT">普通注入</option>
        <option value="BAKIN">Bakin 专用启动</option>
      </select>
      {value === 'BAKIN' && (
        <span className="muted">使用公共 MTool 的 BakinLauncher.exe，工作目录为游戏根目录。</span>
      )}
    </label>
  );
}
