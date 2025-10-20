use tonic::Status;

/// Convert state errors to gRPC Status
pub fn state_error(msg: impl std::fmt::Display) -> Status {
    Status::internal(format!("State error: {}", msg))
}

/// Convert validation errors to gRPC Status
pub fn validation_error(msg: impl std::fmt::Display) -> Status {
    Status::invalid_argument(msg.to_string())
}

