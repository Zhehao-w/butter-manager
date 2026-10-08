import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { fireEvent, render, screen } from '@testing-library/react';
import { EngineSelect } from './EngineSelect';

function Editor({ initial = 'Unknown' }: { initial?: string }) {
  const [engine, setEngine] = useState(initial);
  return (
    <label>
      游戏引擎
      <EngineSelect value={engine} onChange={setEngine} />
    </label>
  );
}
describe('engine selector', () => {
  it('keeps all options available when switching between selected engines', () => {
    render(<Editor />);
    const select = screen.getByRole('combobox', { name: '游戏引擎' }) as HTMLSelectElement;
    fireEvent.change(select, { target: { value: 'QSP' } });
    expect(select.value).toBe('QSP');
    expect(screen.getByRole('option', { name: 'Unity' })).toBeTruthy();
    fireEvent.change(select, { target: { value: 'Unity' } });
    expect(select.value).toBe('Unity');
    expect(screen.queryByRole('textbox')).toBeNull();
  });
  it('retains custom names and can switch back to a known engine', () => {
    render(<Editor initial="Custom Runtime" />);
    const input = screen.getByRole('textbox', { name: '自定义引擎名称' }) as HTMLInputElement;
    expect(input.value).toBe('Custom Runtime');
    fireEvent.change(input, { target: { value: 'Unity' } });
    expect(input.value).toBe('Unity');
    fireEvent.change(screen.getByRole('combobox', { name: '游戏引擎' }), {
      target: { value: 'Godot' },
    });
    expect(screen.queryByRole('textbox')).toBeNull();
  });
});
