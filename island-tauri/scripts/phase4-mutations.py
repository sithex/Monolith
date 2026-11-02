#!/usr/bin/env python3
"""Phase 4 mutation belt. Runtime assertion failures only; restore/touch/diff every source."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
ROOT = Path(__file__).resolve().parents[1]
CRATE = ROOT / 'src-tauri'
EVID = ROOT / 'evidence/phase4/mutations'
CASES = [
 ('loop-guard-removed', 'src/integrations/notifications/store.rs', 'input.app_name == "Hermes Island" || ', '', ['--test','notification_core_store','loop_guard_and_deny_list_drop_without_deltas_or_id_consumption'], 'loop_guard_and_deny_list_drop_without_deltas_or_id_consumption'),
 ('dnd-does-not-suppress-toast', 'src/state/machine.rs', '&& !self.state.dnd', '&& true', ['--test','notification_actor','actor_dnd_prevents_notification_hover_but_counts_arrivals'], 'actor_dnd_prevents_notification_hover_but_counts_arrivals'),
 ('pointer-enter-leaves-toast-timer', 'src/state/machine.rs', 'self.timers.remove(&Timer::ToastTtl);', '', ['--test','notification_actor','pointer_enter_removes_toast_timer_not_just_its_hover_cause'], 'pointer_enter_removes_toast_timer_not_just_its_hover_cause'),
 ('ingest-uid-check-removed', 'src/integrations/notifications/hermes_ingest.rs', 'Ok(peer.uid() == uid)', 'Ok(peer.uid() == uid || true)', ['--lib','ingest_uid_check_uses_kernel_credentials_and_refuses_other_uid'], 'ingest_uid_check_uses_kernel_credentials_and_refuses_other_uid'),
 ('gtk-leave-not-forwarded', 'src/window/crossing.rs', 'if let Some(intent) = intent(false, event.detail()) {', 'if let Some(intent) = None::<Intent> {', ['--test','kwin_phase3_actor','actor_owns_hover_tabs_focus_cues_and_geometry'], 'actor_owns_hover_tabs_focus_cues_and_geometry'),
 ('notify-inferior-not-filtered', 'src/window/crossing.rs', 'if detail == gdk::NotifyType::Inferior {', 'if detail == gdk::NotifyType::Inferior && false {', ['--lib','notify_inferior_is_not_a_boundary_crossing'], 'notify_inferior_is_not_a_boundary_crossing'),
 ('live-pid-temp-reclaimed', 'src/status_temps.rs', '_ => continue,', '_ => {},', ['--lib','live_pid_status_temp_is_never_reclaimed'], 'live_pid_status_temp_is_never_reclaimed'),
 ('image-width-bound-removed', 'src/integrations/notifications/dbus.rs', '|| self.width > MAX_IMAGE_SIDE', '', ['--test','notification_core_dbus','image_width_bound_is_independent_of_stride_height_and_byte_bounds'], 'image_width_bound_is_independent_of_stride_height_and_byte_bounds'),
 ('ingest-payload-cap-removed', 'src/integrations/notifications/ingest.rs', 'if payload.len() > MAX_LINE_BYTES {', 'if payload.len() > MAX_LINE_BYTES && false {', ['--test','notification_core_ingest','line_cap_exact_boundary_counts_bytes_excluding_lf_or_crlf'], 'line_cap_exact_boundary_counts_bytes_excluding_lf_or_crlf'),
 ('monitor-loop-guard-bypassed', 'src/integrations/notifications/linux_monitor.rs', 'if args.app_name == "Hermes Island" {', 'if args.app_name == "Hermes Island" && false {', ['--test','notification_monitor','loop_guard_and_policy_are_enforced_on_private_bus'], 'loop_guard_and_policy_are_enforced_on_private_bus'),
 ('monitor-owner-not-rearmed', 'src/integrations/notifications/linux_monitor.rs', 'if replacement != current {', 'if replacement != current && false {', ['--test','notification_monitor','owner_replacement_rearms_monitor_and_bounds_pending'], 'owner_replacement_rearms_monitor_and_bounds_pending'),
 ('monitor-reply-serial-without-caller', 'src/integrations/notifications/dbus.rs', '.position(|p| p.sender == destination && p.serial == reply_serial)', '.position(|p| p.serial == reply_serial && !destination.is_empty())', ['--test','notification_monitor','monitor_reply_requires_authenticated_owner_and_caller_not_only_serial'], 'monitor_reply_requires_authenticated_owner_and_caller_not_only_serial'),
 ('privacy-overflow-repair-disabled', 'src/integrations/notifications/hub.rs', 'inner.repair_needed = true;', 'inner.repair_needed = false;', ['--test','notification_monitor','privacy_scrub_at_capacity_repairs_renderer_after_delivery_overflow'], 'privacy_scrub_at_capacity_repairs_renderer_after_delivery_overflow'),
]
def main():
    EVID.mkdir(parents=True, exist_ok=True)
    watched = [path for tree in ['src-tauri/src', 'src-tauri/tests', 'ui'] for path in (ROOT/tree).rglob('*') if path.is_file() and '__pycache__' not in path.parts]
    hashes = {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest() for path in watched}
    rows=[]
    with tempfile.TemporaryDirectory(prefix='phase4-mutations-',dir=os.environ['TMPDIR']) as directory:
        pristine=Path(directory)
        sources={name:(CRATE/name).read_bytes() for _,name,*_ in CASES}
        for name,data in sources.items():
            target=pristine/name;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
        try:
            for label,name,old,new,args,expected in CASES:
                path=CRATE/name;text=sources[name].decode()
                assert text.count(old)==1,(label,'nonunique mutation anchor')
                log=EVID/f'{label}.log'
                try:
                    path.write_text(text.replace(old,new,1))
                    with log.open('w') as output:
                        result=subprocess.run(['nice','-n','10','cargo','test','--locked',*args],cwd=CRATE,stdout=output,stderr=subprocess.STDOUT,timeout=180)
                    output=log.read_text()
                    assert result.returncode != 0 and f'{expected} ... FAILED' in output and 'could not compile' not in output,(label,'no named runtime failure',output[-3000:])
                    failed=[line for line in output.splitlines() if line.startswith('test ') and line.endswith('FAILED')]
                finally:
                    path.write_bytes(sources[name]);os.utime(path,None)
                    result=subprocess.run(['diff','-u',str(pristine/name),str(path)],capture_output=True,text=True)
                    (EVID/f'{label}.diff.log').write_text(result.stdout+result.stderr)
                    assert result.returncode==0,(label,'restore differs')
                rows.append({'mutation':label,'failedTests':failed,'restored':True})
                print(json.dumps(rows[-1]),flush=True)
        finally:
            for name,data in sources.items():
                (CRATE/name).write_bytes(data);os.utime(CRATE/name,None)
        assert all((CRATE/name).read_bytes()==data for name,data in sources.items())
    with (EVID/'restored-tests.log').open('w') as output:
        # One cargo command and one GUI test binary at a time (bounded build memory).
        for target in ['--lib', 'notification_actor', 'notification_core_store', 'notification_core_dbus', 'notification_core_ingest', 'notification_core_edges', 'kwin_phase3_actor', 'kwin_phase4_notifications', 'kwin_phase4_tray', 'notification_monitor']:
            args = ['--lib'] if target == '--lib' else ['--test', target]
            result=subprocess.run(['nice','-n','10','cargo','test','--locked',*args],cwd=CRATE,stdout=output,stderr=subprocess.STDOUT,timeout=180)
            assert result.returncode==0, f'restored tests failed: {target}'
    assert all(hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==value for name,value in hashes.items()),'source changed during mutations'
    (EVID/'source-hashes.json').write_text(json.dumps(hashes,indent=2,sort_keys=True)+'\n')
    (EVID/'summary.json').write_text(json.dumps(rows,indent=2)+'\n')
    print(f'PASS: {len(rows)} compiling runtime mutations killed, named distinct tests; every restoration diff-identical; restored tests passed')
if __name__=='__main__': main()
