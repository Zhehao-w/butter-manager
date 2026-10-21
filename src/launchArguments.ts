// Windows-style argument quoting; this field never contains a shell command or EXE path.
export function parseLaunchArguments(value: string): string[] {
  if (/[\x00-\x08\x0a-\x1f\x7f-\x9f]/.test(value)) {
    throw new Error('启动参数不能包含换行或控制字符');
  }
  const args: string[] = [];
  let argument = '';
  let quoted = false;
  let started = false;
  let index = 0;
  while (index < value.length) {
    const char = value[index];
    if (char === '\\') {
      const start = index;
      while (value[index] === '\\') index++;
      const count = index - start;
      if (value[index] === '"') {
        argument += '\\'.repeat(Math.floor(count / 2));
        started = true;
        if (count % 2) {
          argument += '"';
          index++;
        }
      } else {
        argument += '\\'.repeat(count);
        started = true;
      }
    } else if (char === '"') {
      started = true;
      if (quoted && value[index + 1] === '"') {
        argument += '"';
        index += 2;
      } else {
        quoted = !quoted;
        index++;
      }
    } else if (!quoted && (char === ' ' || char === '\t')) {
      if (started) {
        args.push(argument);
        argument = '';
        started = false;
      }
      index++;
    } else {
      argument += char;
      started = true;
      index++;
    }
  }
  if (quoted) throw new Error('启动参数的双引号未闭合，请补全双引号');
  if (started) args.push(argument);
  return args;
}
export function launchArgumentsError(value: string): string {
  try {
    parseLaunchArguments(value);
    return '';
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
}
