declare module '*.png' {
  const url: string;
  export default url;
}

declare module '*.svg' {
  const url: string;
  export default url;
}
declare module '*?raw' {
  const text: string;
  export default text;
}
