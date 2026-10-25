/* Toast: tiny module-level pub/sub so any module can toast without context. */

type Handler = (msg: string) => void;
let handler: Handler = () => {};

export function onToast(h: Handler): void { handler = h; }
export function toast(msg: string): void { handler(msg); }
