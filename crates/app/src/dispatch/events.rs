use crate::{ParcelCacheInvalidation, UiEvent};

pub(super) fn text_events(text: String, extra: Option<UiEvent>) -> Vec<UiEvent> {
    let mut events = vec![UiEvent::Text(text)];
    if let Some(event) = extra {
        events.push(event);
    }
    events
}

pub(super) fn parcel_cache_event(cache: ParcelCacheInvalidation) -> UiEvent {
    UiEvent::InvalidateParcelCache {
        view_id: cache.view_id,
        front_view_id: cache.front_view_id,
    }
}
