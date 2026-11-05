// Album scheme runtime (Nov 5): samples the playing track's artwork ONCE per artwork change
// (24x24 canvas downsample → pickAlbumColour) and shares the resulting palette with the
// Settings preview through a tiny store. No per-frame work: the crossfade between tracks
// is a CSS transition on registered `--album-*` properties (polish.css).
import { useEffect, useSyncExternalStore } from 'react';
import { ALBUM_SAMPLE, albumPalette, pickAlbumColour } from './tokens/album';
import type { SchemeRoles } from './tokens/schemes';

const ART = /^data:image\/(?:png|jpeg|webp);base64,[A-Za-z0-9+/=]+$/;
let sampled: { art: string; colour: Promise<string | null> } | null = null;

/** Dominant vivid colour of a (service-proxied, data: URL) artwork; memoized for the last artwork. */
export function sampleArtwork(art: string): Promise<string | null> {
  if (sampled?.art === art) return sampled.colour;
  const colour = new Promise<string | null>(resolve => {
    if (art.length > 1400000 || !ART.test(art)) { resolve(null); return; }
    const image = new Image();
    image.onload = () => {
      try {
        const canvas = document.createElement('canvas');
        canvas.width = canvas.height = ALBUM_SAMPLE;
        const context = canvas.getContext('2d', { willReadFrequently: true });
        if (!context) { resolve(null); return; }
        context.drawImage(image, 0, 0, ALBUM_SAMPLE, ALBUM_SAMPLE);
        resolve(pickAlbumColour(context.getImageData(0, 0, ALBUM_SAMPLE, ALBUM_SAMPLE).data));
      } catch { resolve(null); }
    };
    image.onerror = () => resolve(null);
    image.src = art;
  });
  sampled = { art, colour };
  return colour;
}

// The palette currently derived from artwork (null = no artwork → Skin fallback).
let current: SchemeRoles | null = null;
const listeners = new Set<() => void>();
const publish = (next: SchemeRoles | null) => {
  if (JSON.stringify(next) === JSON.stringify(current)) return;
  current = next; for (const listener of listeners) listener();
};
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
/** Read-only: the album palette for previews (Settings → Colour scheme). */
export const useAlbumPreview = () => useSyncExternalStore(subscribe, () => current, () => null);

/**
 * Island root only: keep the album palette in sync with `art` (the now-playing artwork).
 * Sampling runs while the Album scheme is chosen or `preview` (Settings open) asks for the
 * swatch; the last result stays published. Returns the palette to apply, or null for the Skin fallback.
 */
export function useAlbumScheme(enabled: boolean, art: string | null, preview = false): SchemeRoles | null {
  const sample = enabled || preview;
  useEffect(() => {
    if (!sample) return;
    if (!art) { publish(null); return; }
    let live = true;
    void sampleArtwork(art).then(colour => { if (live) publish(colour ? albumPalette(colour) : null); });
    return () => { live = false; };
  }, [sample, art]);
  const palette = useAlbumPreview();
  return enabled ? palette : null;
}
