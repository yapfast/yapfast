//! The call's connection in detail: the ping as the signal icon's tooltip,
//! and the popout's graph, numbers and voice privacy code, and how a shared
//! screen goes out. The call measures them all along; they're copied out of
//! it only while one of them shows.

use yapfast_core::voice::LinkStats;

use super::*;
use yapfast_core::call_details::{
    average, encoder, graph, groups, milliseconds, path, picture, rate,
};

pub(super) fn bind(window: &YapfastWindow, adapter: &Rc<RefCell<Adapter>>) {
    let state = Rc::downgrade(adapter);
    let weak = window.as_weak();
    window.global::<CallLink>().on_read(move || {
        let (Some(adapter), Some(window)) = (state.upgrade(), weak.upgrade()) else {
            return;
        };
        let Ok(adapter) = adapter.try_borrow() else {
            return;
        };
        let voice = &adapter.app.state.voice;
        let stats = voice
            .link
            .as_ref()
            .map(yapfast_core::voice::Link::read)
            .unwrap_or_default();
        show(
            &window.global::<CallLink>(),
            &stats,
            voice.participants.len() > 1,
        );
    });
}

/// Shows `stats`, with the privacy code only while `others` are in the call
/// to compare it with, as Discord's client does.
fn show(link: &CallLink<'_>, stats: &LinkStats, others: bool) {
    let last = yapfast_core::call_details::last_ping(stats);
    link.set_ping(last.unwrap_or_default().into());
    let average = average(&stats.pings).map(milliseconds);
    link.set_average(average.unwrap_or_default().into());
    let loss = yapfast_core::call_details::loss(stats);
    link.set_loss(loss.unwrap_or_default().into());
    let (steps, top) = graph(&stats.pings);
    link.set_graph(path(&steps).into());
    link.set_graph_top(top as i32);
    let code = stats.privacy_code.as_deref().filter(|_| others);
    let groups: Vec<slint::SharedString> = code
        .map(groups)
        .unwrap_or_default()
        .into_iter()
        .map(Into::into)
        .collect();
    let shown = link.get_code();
    if !shown.iter().eq(groups.iter().cloned()) {
        link.set_code(slint::ModelRc::new(slint::VecModel::from(groups)));
    }
    link.set_streaming(stats.stream.is_some());
    if let Some(stream) = &stats.stream {
        link.set_encoder(encoder(&stream.engine).into());
        link.set_codec(stream.codec.as_str().into());
        link.set_picture(picture(stream).into());
        link.set_sent(rate(stream.sent_kbps).into());
        link.set_target(rate(stream.target_kbps).into());
        link.set_keyframes(stream.keyframe_requests.to_string().into());
        link.set_resent(stream.resent.to_string().into());
    }
}
