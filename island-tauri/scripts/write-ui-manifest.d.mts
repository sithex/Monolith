export declare const MANIFEST_NAME: string;
export declare function contractHash(appRoot?: string): string;
export declare function hashTree(dist: string): Record<string, string>;
export declare function writeManifest(dist?: string, appRoot?: string): { version: number; contractHash: string; generatedAt: string; files: Record<string, string> };
export default function uiManifestPlugin(): import('vite').Plugin;
