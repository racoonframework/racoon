///
/// More information: <https://developer.mozilla.org/en-US/docs/Web/HTTP/Status>
///
pub trait ResponseStatus: Sized {
    fn with_status(status_code: u32, status_text: &str) -> Self;

    fn r#continue() -> Self {
        Self::with_status(100, "Continue")
    }

    fn switching_protocols() -> Self {
        Self::with_status(101, "Switching Protocols")
    }

    fn processing() -> Self {
        Self::with_status(102, "Processing")
    }

    fn early_hints() -> Self {
        Self::with_status(103, "Early Hints")
    }

    fn ok() -> Self {
        Self::with_status(200, "OK")
    }

    fn created() -> Self {
        Self::with_status(201, "Created")
    }

    fn accepted() -> Self {
        Self::with_status(202, "Accepted")
    }

    fn non_authoritative_information() -> Self {
        Self::with_status(203, "Non-Authoritative Information")
    }

    fn no_content() -> Self {
        Self::with_status(204, "No Content")
    }

    fn reset_content() -> Self {
        Self::with_status(205, "Reset Content")
    }

    fn partial_content() -> Self {
        Self::with_status(206, "Partial Content")
    }

    fn multi_status() -> Self {
        Self::with_status(207, "Multi-Status")
    }

    fn already_reported() -> Self {
        Self::with_status(208, "Already Reported")
    }

    fn im_used() -> Self {
        Self::with_status(226, "IM Used")
    }

    fn multiple_choices() -> Self {
        Self::with_status(300, "Multiple Choices")
    }

    fn moved_permanently() -> Self {
        Self::with_status(301, "Moved Permanently")
    }

    fn found() -> Self {
        Self::with_status(302, "Found")
    }

    fn see_other() -> Self {
        Self::with_status(303, "See Other")
    }

    fn not_modified() -> Self {
        Self::with_status(304, "Not Modified")
    }

    ///
    /// Depreciated
    ///
    fn use_proxy() -> Self {
        Self::with_status(305, "Use Proxy")
    }

    ///
    /// Depreciated
    ///
    fn unused() -> Self {
        Self::with_status(306, "Unused")
    }

    fn temporary_redirect() -> Self {
        Self::with_status(307, "Temporary Redirect")
    }

    fn permanent_redirect() -> Self {
        Self::with_status(308, "Permanent Redirect")
    }

    fn bad_request() -> Self {
        Self::with_status(400, "Bad Request")
    }

    fn unauthorized() -> Self {
        Self::with_status(401, "Unauthorized")
    }

    ///
    /// Experimental. Expect behaviour to change in the future.
    ///
    fn payment_required() -> Self {
        Self::with_status(403, "Payment Required")
    }

    fn forbidden() -> Self {
        Self::with_status(403, "Forbidden")
    }

    fn not_found() -> Self {
        Self::with_status(404, "Not Found")
    }

    fn method_not_allowed() -> Self {
        Self::with_status(405, "Method Not Allowed")
    }

    fn not_acceptable() -> Self {
        Self::with_status(406, "Not Acceptable")
    }

    fn proxy_authentication_required() -> Self {
        Self::with_status(407, "Proxy Authentication Required")
    }

    fn request_timeout() -> Self {
        Self::with_status(408, "Request Timeout")
    }

    fn conflict() -> Self {
        Self::with_status(409, "Conflict")
    }

    fn gone() -> Self {
        Self::with_status(410, "Gone")
    }

    fn length_required() -> Self {
        Self::with_status(411, "Length Required")
    }

    fn precondition_failed() -> Self {
        Self::with_status(412, "Precondition Failed")
    }

    fn payload_too_large() -> Self {
        Self::with_status(412, "Payload Too Large")
    }

    fn uri_too_long() -> Self {
        Self::with_status(414, "URI Too Long")
    }

    fn unsupported_media_type() -> Self {
        Self::with_status(415, "Unsupported Media Type")
    }

    fn range_not_satisfiable() -> Self {
        Self::with_status(416, "Range Not Satisfiable")
    }

    fn expectation_failed() -> Self {
        Self::with_status(417, "Expectation Failed")
    }

    fn im_a_teapot() -> Self {
        Self::with_status(418, "I',m a teapot")
    }

    fn misdirected_request() -> Self {
        Self::with_status(421, "Misdirected Request")
    }

    fn unprocessable_content() -> Self {
        Self::with_status(422, "Unprocessable Content")
    }

    fn locked() -> Self {
        Self::with_status(423, "Locked")
    }

    fn failed_dependency() -> Self {
        Self::with_status(424, "Failed Dependency")
    }


    ///
    /// Experimental. Expect behaviour to change in the future.
    ///
    fn too_early() -> Self {
        Self::with_status(425, "Too Early")
    }

    fn upgrade_required() -> Self {
        Self::with_status(426, "Upgrade Required")
    }

    fn precondition_required() -> Self {
        Self::with_status(428, "Precondition Required")
    }

    fn too_many_requests() -> Self {
        Self::with_status(429, "Too Many Requests")
    }

    fn request_header_fields_too_large() -> Self {
        Self::with_status(431, "Request Header Fields Too Large")
    }

    fn unavailable_for_legal_reasons() -> Self {
        Self::with_status(451, "Unavailable For Legal Reasons")
    }

    fn internal_server_error() -> Self {
        Self::with_status(500, "Internal Server Error")
    }

    fn not_implemented() -> Self {
        Self::with_status(501, "Not Implemented")
    }

    fn bad_gateway() -> Self {
        Self::with_status(502, "Bad Gateway")
    }

    fn service_unavailable() -> Self {
        Self::with_status(503, "Service Unavailable")
    }

    fn gateway_timeout() -> Self {
        Self::with_status(504, "Gateway Timeout")
    }

    fn http_version_not_supported() -> Self {
        Self::with_status(505, "HTTP Version Not Supported")
    }

    fn variant_also_negotiates() -> Self {
        Self::with_status(506, "Variant Also Negotiates")
    }

    fn insufficient_storage() -> Self {
        Self::with_status(507, "Insufficient Storage")
    }

    fn loop_detected() -> Self {
        Self::with_status(508, "Loop Detected")
    }

    fn not_extended() -> Self {
        Self::with_status(510, "Not Extended")
    }

    fn network_authentication_required() -> Self {
        Self::with_status(511, "Network Authentication Required")
    }
}