export const supportedEngines: string[] = [
  'Unity',
  "Ren'Py",
  'RPG Maker MV',
  'RPG Maker MZ',
  'RPG Maker (legacy)',
  'Godot',
  'RPG Developer Bakin',
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

export const engineLabel = (engine: string) =>
  engine === 'RPG Developer Bakin' ? 'Bakin' : engine === 'Unknown' ? '未识别' : engine;
