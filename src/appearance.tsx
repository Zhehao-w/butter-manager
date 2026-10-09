import { useRef, useState } from 'react';
import type { Appearance, AppearanceChoice } from './types';
import originalIcon from './assets/app-icon.png';
import newIcon from './assets/app-icon-new.png';
import originalIllustration from './assets/sidebar-character.png';
import newIllustration from './assets/sidebar-character-new.png';

export const defaultAppearance: Appearance = { icon: 'new', illustration: 'new' };
export const appearanceIcons = { original: originalIcon, new: newIcon };
export const appearanceIllustrations = { original: originalIllustration, new: newIllustration };

export function AppearancePicker({
  value,
  disabled,
  onSave,
}: {
  value: Appearance;
  disabled: boolean;
  onSave: (next: Appearance) => Promise<void>;
}) {
  const [busy, setBusy] = useState(false);
  const saving = useRef(false);
  const [error, setError] = useState('');
  async function select(field: keyof Appearance, choice: AppearanceChoice) {
    if (disabled || saving.current || value[field] === choice) return;
    saving.current = true;
    setBusy(true);
    setError('');
    try {
      await onSave({ ...value, [field]: choice });
    } catch (reason) {
      setError(String(reason));
    } finally {
      saving.current = false;
      setBusy(false);
    }
  }
  return (
    <section className="appearance-card panel" aria-label="外观设置">
      <h3>外观</h3>
      <p className="muted">选择后自动保存。应用内与窗口图标同步切换，EXE 文件图标使用新版。</p>
      <div className="appearance-groups">
        {(['icon', 'illustration'] as const).map((field) => {
          const label = field === 'icon' ? '应用图标' : '侧栏立绘';
          const images = field === 'icon' ? appearanceIcons : appearanceIllustrations;
          return (
            <fieldset className="appearance-group" disabled={disabled || busy} key={field}>
              <legend>{label}</legend>
              <div className="appearance-options">
                {(['original', 'new'] as const).map((choice) => (
                  <label
                    className={`appearance-option appearance-${field} ${value[field] === choice ? 'selected' : ''}`}
                    key={choice}
                  >
                    <input
                      type="radio"
                      name={`appearance-${field}`}
                      aria-label={`${choice === 'original' ? '原版' : '新版'}${label}`}
                      checked={value[field] === choice}
                      onChange={() => void select(field, choice)}
                    />
                    <span className="appearance-preview">
                      <img src={images[choice]} alt="" />
                    </span>
                    <span>{choice === 'original' ? '原版' : '新版'}</span>
                  </label>
                ))}
              </div>
            </fieldset>
          );
        })}
      </div>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
