use crate::{CommercialParcelCacheInvalidation, UiEvent};

pub(super) fn text_events(text: String, extra: Option<UiEvent>) -> Vec<UiEvent> {
    let mut events = vec![UiEvent::Text(text)];
    if let Some(event) = extra {
        events.push(event);
    }
    events
}

pub(super) fn commercial_parcel_cache_event(cache: CommercialParcelCacheInvalidation) -> UiEvent {
    UiEvent::InvalidateCommercialParcelCache {
        view_id: cache.view_id,
        front_view_id: cache.front_view_id,
    }
}
