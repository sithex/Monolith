//! Wayland callbacks only update loop-owned state; no blocking receive/IO here.
use super::*;
use wayland_client::{delegate_noop, event_created_child, Dispatch};

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => {
                state.globals = state.globals.saturating_add(1);
                if state.globals > 256 {
                    state.failed = true;
                    return;
                }
                match interface.as_str() {
                    "ext_data_control_manager_v1" if state.ext.is_none() => {
                        state.ext = Some((name, version))
                    }
                    "zwlr_data_control_manager_v1" if state.wlr.is_none() => {
                        state.wlr = Some((name, version))
                    }
                    "wl_seat" if state.seat.is_none() => state.seat = Some((name, version)),
                    _ => {}
                }
            }
            wl_registry::Event::GlobalRemove { name }
                if [state.ext, state.wlr, state.seat]
                    .iter()
                    .flatten()
                    .any(|(n, _)| *n == name) =>
            {
                state.failed = true;
            }
            _ => {}
        }
    }
}
impl Dispatch<wl_callback::WlCallback, Sync> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        sync: &Sync,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event {
            match sync {
                Sync::Registry => state.registry_done = true,
                Sync::Metadata(seq) => {
                    if let Some(s) = state.selected.as_mut().filter(|s| s.seq == *seq) {
                        s.metadata_done = true;
                    }
                }
                Sync::Commit(seq) => {
                    if let Some(s) = state.selected.as_mut().filter(|s| s.seq == *seq) {
                        s.commit_done = true;
                    }
                }
                Sync::Fence(epoch) => {
                    if state.shared.epoch.load(Ordering::Acquire) == *epoch {
                        state.capture_ready = state.shared.active(*epoch);
                    }
                }
            }
        }
    }
}
delegate_noop!(State: ignore wl_seat::WlSeat);
delegate_noop!(State: ignore ExtManager);
delegate_noop!(State: ignore WlrManager);

// Both protocols have the same wire semantics but distinct proxy types. Generate
// only the callback glue; all policy, transfer bounds and lifetime rules live once.
macro_rules! callbacks {
    ($device:ident,$device_mod:ident,$offer:ident,$offer_mod:ident,$source:ident,$source_mod:ident,$variant:ident)=>{
        impl Dispatch<$device,()> for State {
            fn event(state:&mut Self,_:&$device,event:$device_mod::Event,_:&(),connection:&Connection,qh:&QueueHandle<Self>) {
                match event {
                    $device_mod::Event::DataOffer{id}=>state.new_offer(Offer::$variant(id)),
                    $device_mod::Event::Selection{id}=>state.select(id.map(Offer::$variant),connection,qh),
                    $device_mod::Event::PrimarySelection{id:Some(offer)}=>{
                        let id=offer.id().protocol_id();if state.selected.as_ref().is_none_or(|s|s.id!=id){state.forget(id);}
                    }
                    $device_mod::Event::Finished=>state.failed=true,
                    _=>{},
                }
            }
            event_created_child!(State,$device,[0=>($offer,())]);
        }
        impl Dispatch<$offer,()> for State {
            fn event(state:&mut Self,offer:&$offer,event:$offer_mod::Event,_:&(),_:&Connection,_:&QueueHandle<Self>) {
                if let $offer_mod::Event::Offer{mime_type}=event {state.mime(offer.id().protocol_id(),mime_type);}
            }
        }
        impl Dispatch<$source,Arc<SendData>> for State {
            fn event(state:&mut Self,source:&$source,event:$source_mod::Event,data:&Arc<SendData>,_:&Connection,_:&QueueHandle<Self>) {
                match event {
                    $source_mod::Event::Send{mime_type,fd}=>{
                        if state.source.as_ref().is_some_and(|s|s.id()==source.id().protocol_id()) && state.shared.enabled.load(Ordering::Acquire) {state.send(&mime_type,fd,data);}
                    }
                    $source_mod::Event::Cancelled if state.source.as_ref().is_some_and(|s|s.id()==source.id().protocol_id())=>{
                        if let Some(source)=state.source.take(){source.destroy();}
                    }
                    _=>{},
                }
            }
        }
    };
}
callbacks!(
    ExtDevice,
    ext_data_control_device_v1,
    ExtOffer,
    ext_data_control_offer_v1,
    ExtSource,
    ext_data_control_source_v1,
    Ext
);
callbacks!(
    WlrDevice,
    zwlr_data_control_device_v1,
    WlrOffer,
    zwlr_data_control_offer_v1,
    WlrSource,
    zwlr_data_control_source_v1,
    Wlr
);
