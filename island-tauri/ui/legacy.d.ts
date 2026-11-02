// The fork renderer is plain JS/JSX copied byte-for-byte (SPEC §6); it is not type-checked.
declare module "*.jsx";
declare module "*.cjs";
declare module "@xterm/xterm/lib/xterm.js" {
  export { Terminal } from "@xterm/xterm";
}
