import { join } from 'path';
import { logForDebugging } from '../../utils/debug.js';

interface NativeBindings {
  globSearch(options: { dir: string; pattern: string; maxResults?: number }): Promise<string[]>;
  grepSearch(options: { dir: string; pattern: string; includePattern?: string; maxResults?: number }): Promise<string[]>;
  applyFileEdit(options: { absolutePath: string; oldString: string; newString: string }): Promise<string>;
}

let nativeBindings: NativeBindings | null = null;

try {
  // Dynamically load the .node binary if compiled
  // Using dynamic require to avoid bundling issues
  const { createRequire } = require('module');
  const req = createRequire(__filename);
  nativeBindings = req(join(process.cwd(), 'claude-native', 'claude-native.node'));
  logForDebugging('[N-API] Native Rust bindings loaded successfully.');
} catch (e) {
  logForDebugging(`[N-API] Native Rust bindings not found, falling back to TypeScript/Subprocess implementations. ${e}`);
}

export function getNativeBindings(): NativeBindings | null {
  return nativeBindings;
}
