/// Result from creating a shop payment request.
pub struct ShopPaymentRequestResult<I> {
    /// Text to display to the shop operator.
    pub text: String,
    /// Payer player id for live delivery.
    pub payer_player_id: String,
    /// Inbox item generated for the payer.
    pub inbox_item: I,
}

/// Result from a direct payment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayDirectResult<T> {
    /// Text to display to the payer.
    pub text: String,
    /// Transfer details for optional live delivery.
    pub transfer: T,
}

/// Result from accepting a payment request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayAcceptResult<R> {
    /// Text to display to the payer.
    pub text: String,
    /// Paid request details for optional live delivery.
    pub request: R,
}

/// Result from a read-only business listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusinessListResult {
    /// Text to display to the user.
    pub text: String,
}

/// Result from a land operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LandCommandResult {
    /// Text to display to the user.
    pub text: String,
    /// Optional commercial parcel cache invalidation.
    pub invalidate: Option<CommercialParcelCacheInvalidation>,
}

/// Result from a build operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BuildCommandResult {
    /// Text to display to the user.
    pub text: String,
    /// Optional commercial parcel cache invalidation.
    pub invalidate: Option<CommercialParcelCacheInvalidation>,
}

/// Cache key for a commercial parcel and its front view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommercialParcelCacheInvalidation {
    /// Parcel view id.
    pub view_id: String,
    /// Front/street view where the parcel is visible.
    pub front_view_id: String,
}

/// Default custom commands for a parcel build sheet when the owner omits commands.
#[must_use]
pub const fn default_build_commands() -> &'static str {
    "/hello preview=hello price=25; /status"
}

/// Help text for parcel build commands.
#[must_use]
pub const fn build_help_text() -> &'static str {
    "Build commands for the current owned parcel:\r\n\
     /build {\"title\":\"shop title\",\"description\":\"shop description\",\"style\":\"style note\",\"prompt\":\"operator prompt\"}\r\n\
     Optional JSON field: \"commands\". If omitted, commands are auto-filled.\r\n\
     Legacy field commands still work for manual correction: /build title <text>, /build description <text>, /build style <text>, /build prompt <text>, /build commands <text>\r\n\
     /build publish\r\n\
     After publishing, visitor slash commands inside the shop become inbox items for the owner.\r\n"
}
