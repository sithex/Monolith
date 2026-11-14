import type { QuickInfoModel } from '../app-presentations';
import type { IconName } from '../models';
import { Empty, ServiceCard } from './AppPrimitives';
import { Icon } from './Icon';
const conditions: Record<number, string> = { 0: 'Clear sky', 1: 'Mainly clear', 2: 'Partly cloudy', 3: 'Overcast', 45: 'Fog', 48: 'Depositing rime fog', 51: 'Light drizzle', 53: 'Moderate drizzle', 55: 'Dense drizzle', 56: 'Light freezing drizzle', 57: 'Dense freezing drizzle', 61: 'Slight rain', 63: 'Moderate rain', 65: 'Heavy rain', 66: 'Light freezing rain', 67: 'Heavy freezing rain', 71: 'Slight snowfall', 73: 'Moderate snowfall', 75: 'Heavy snowfall', 77: 'Snow grains', 80: 'Slight rain showers', 81: 'Moderate rain showers', 82: 'Violent rain showers', 85: 'Slight snow showers', 86: 'Heavy snow showers', 95: 'Thunderstorm', 96: 'Thunderstorm with slight hail', 97: 'Heavy thunderstorm', 99: 'Thunderstorm with heavy hail' };
function weatherIcon(code: number | null, night: boolean): IconName {
  if (code === null || !conditions[code]) return 'weather';
  if (code <= 1) return night ? 'moon' : 'sun';
  if (code >= 95) return 'storm';
  if (code >= 71 && code <= 86 && ![80, 81, 82].includes(code)) return 'snow';
  if (code >= 51) return 'rain';
  return 'cloud';
}
const powerLabels: Record<string, string> = { charging: 'Charging', fullyCharged: 'Fully charged', discharging: 'On battery', unknown: 'Charge state unknown', empty: 'Empty', pendingCharge: 'Pending charge', pendingDischarge: 'Pending discharge' };
export function QuickInfo({ model }: { model: QuickInfoModel }) {
  return <div className="app-stack quick-info"><ServiceCard title="Weather" icon="weather" source="weather" service={model.weather}>{weather => <><div className="weather-reading"><span aria-label={weather.code !== null && conditions[weather.code] ? 'Weather condition' : 'Generic weather icon'}><Icon name={weatherIcon(weather.code, weather.isDay === false)}/></span><strong>{weather.temp === null ? 'Unknown temperature' : `${weather.temp}°${weather.unit === 'fahrenheit' ? 'F' : 'C'}`}</strong></div><p>{weather.code === null ? 'Unknown condition' : conditions[weather.code] ?? `Unknown condition · WMO ${weather.code}`}</p><small className="muted">{weather.place?.name ?? 'Location unavailable'}{weather.updatedLabel && ` · ${weather.updatedLabel}`}{weather.stale && ' · Stale'}</small></>}</ServiceCard>
    <ServiceCard title="Battery" icon="power" service={model.power}>{power => !power.present ? <p className="muted">No battery present</p> : <><div className="weather-reading"><Icon name="power"/><strong>{power.percent === null ? 'Unknown charge' : `${power.percent}%`}</strong></div><p>{powerLabels[power.state] ?? 'Charge state unknown'}</p></>}</ServiceCard>
    <ServiceCard title="Bluetooth" icon="bluetooth" service={model.bluetooth}>{devices => devices.length ? <ul className="app-list">{devices.map(device => <li className="bluetooth-row" key={device.path}><Icon name="bluetooth"/><div className="row-content"><strong>{device.name}</strong><small>{device.connected ? 'Connected' : device.paired ? 'Paired · Disconnected' : 'Disconnected'}</small></div><span>{device.batteryPercent === null ? 'Battery unknown' : `${device.batteryPercent}%`}</span></li>)}</ul> : <Empty icon="bluetooth">No Bluetooth devices</Empty>}</ServiceCard>
  </div>;
}
