import type { ReactNode } from 'react';
import { Icon } from './Icon';
import type { IconName, Service } from '../models';
export function Empty({ icon, children }: { icon: IconName; children: ReactNode }) {
  return <div className="app-empty"><Icon name={icon}/><p>{children}</p></div>;
}
export function ServiceCard<T>({ title, service, children }: { title: string; service: Service<T>; children(value: T): ReactNode }) {
  return <section className="card"><h3>{title}</h3>{service.available ? children(service.value) : <p className="unavailable">Unavailable · {service.reason}</p>}</section>;
}
// No external image fetches or file:// access from staging. Phase 7 supplies
// bounded service-owned PNGs (including proxied artwork), not arbitrary URLs.
export function SafeImage({ src, alt, className = '' }: { src: string | null; alt: string; className?: string }) {
  if (!src || src.length > 1400000 || !/^data:image\/png;base64,[A-Za-z0-9+/=]+$/.test(src)) return null;
  return <img className={`bounded-image ${className}`} src={src} alt={alt} loading="lazy" referrerPolicy="no-referrer"/>;
}
export function formatSize(bytes: number) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']; let value = Math.max(0, bytes); let unit = 0;
  while (value >= 1024 && unit < units.length - 1) { value /= 1024; unit++; }
  return `${unit ? (Math.floor(value * 10) / 10).toFixed(1) : value} ${units[unit]}`;
}
