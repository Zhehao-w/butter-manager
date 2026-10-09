import { useState } from 'react';
import { Icon } from './ui';

const engines = [
  'Unity',
  "Ren'Py",
  'RPG Maker MV',
  'RPG Maker MZ',
  'RPG Maker (legacy)',
  'Godot',
  'WOLF RPG Editor',
  'KiriKiri',
  'NScripter',
  'TyranoScript',
  'Twine',
  'NW.js',
  'HTML',
  'QSP',
  'Unknown',
];
const customOption = '__custom_engine__';

/** A normal select keeps every engine available after selection; custom names remain editable. */
export function EngineSelect({
  value,
  onChange,
  disabled = false,
  label = '游戏引擎',
}: {
  value: string;
  onChange: (engine: string) => void;
  disabled?: boolean;
  label?: string;
}) {
  const [custom, setCustom] = useState(!engines.includes(value));
  const isCustom = custom || !engines.includes(value);
  return (
    <span className="engine-control">
      <span className="engine-select">
        <select
          aria-label={label}
          value={isCustom ? customOption : value}
          disabled={disabled}
          onChange={(event) => {
            const next = event.target.value;
            setCustom(next === customOption);
            if (next !== customOption) onChange(next);
          }}
        >
          {engines.map((engine) => (
            <option key={engine} value={engine}>
              {engine === 'Unknown' ? '未识别' : engine}
            </option>
          ))}
          <option value={customOption}>自定义引擎…</option>
        </select>
        <Icon name="next" size={16} />
      </span>
      {isCustom && (
        <input
          aria-label="自定义引擎名称"
          value={value === 'Unknown' ? '' : value}
          placeholder="填写引擎名称"
          maxLength={80}
          disabled={disabled}
          onChange={(event) => onChange(event.target.value || 'Unknown')}
        />
      )}
    </span>
  );
}
