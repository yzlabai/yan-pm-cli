use yan_pm_cli::agent::state::{AgentErrorCode, ConnectionState};

#[test]
fn test_normal_lifecycle() {
    let idle = ConnectionState::Idle;

    // Idle → Connecting
    let t1 = idle.transition(ConnectionState::Connecting, None).unwrap();
    assert_eq!(t1.from, ConnectionState::Idle);
    assert_eq!(t1.to, ConnectionState::Connecting);

    // Connecting → Ready
    let t2 = ConnectionState::Connecting
        .transition(ConnectionState::Ready, None)
        .unwrap();
    assert_eq!(t2.from, ConnectionState::Connecting);
    assert_eq!(t2.to, ConnectionState::Ready);

    // Ready → Stopped
    let t3 = ConnectionState::Ready
        .transition(ConnectionState::Stopped, None)
        .unwrap();
    assert_eq!(t3.from, ConnectionState::Ready);
    assert_eq!(t3.to, ConnectionState::Stopped);
}

#[test]
fn test_error_and_retry() {
    // Connecting → Error(Timeout)
    let t1 = ConnectionState::Connecting
        .transition(
            ConnectionState::Error,
            Some(AgentErrorCode::AgentTimeout),
        )
        .unwrap();
    assert_eq!(t1.from, ConnectionState::Connecting);
    assert_eq!(t1.to, ConnectionState::Error);
    assert_eq!(t1.error_code, Some(AgentErrorCode::AgentTimeout));

    // Error → Connecting (retry)
    let t2 = ConnectionState::Error
        .transition(ConnectionState::Connecting, None)
        .unwrap();
    assert_eq!(t2.from, ConnectionState::Error);
    assert_eq!(t2.to, ConnectionState::Connecting);
    assert_eq!(t2.error_code, None);

    // Error → Stopped (give up)
    let t3 = ConnectionState::Error
        .transition(ConnectionState::Stopped, None)
        .unwrap();
    assert_eq!(t3.from, ConnectionState::Error);
    assert_eq!(t3.to, ConnectionState::Stopped);
}

#[test]
fn test_ready_to_error() {
    let t = ConnectionState::Ready
        .transition(
            ConnectionState::Error,
            Some(AgentErrorCode::AgentCrashed),
        )
        .unwrap();
    assert_eq!(t.from, ConnectionState::Ready);
    assert_eq!(t.to, ConnectionState::Error);
    assert_eq!(t.error_code, Some(AgentErrorCode::AgentCrashed));
}

#[test]
fn test_invalid_transitions() {
    // Idle → Ready (skip Connecting)
    assert!(
        ConnectionState::Idle
            .transition(ConnectionState::Ready, None)
            .is_none(),
        "Idle→Ready should be invalid"
    );

    // Idle → Error
    assert!(
        ConnectionState::Idle
            .transition(ConnectionState::Error, None)
            .is_none(),
        "Idle→Error should be invalid"
    );

    // Stopped → Connecting
    assert!(
        ConnectionState::Stopped
            .transition(ConnectionState::Connecting, None)
            .is_none(),
        "Stopped→Connecting should be invalid"
    );

    // Stopped → Ready
    assert!(
        ConnectionState::Stopped
            .transition(ConnectionState::Ready, None)
            .is_none(),
        "Stopped→Ready should be invalid"
    );

    // Stopped → Idle
    assert!(
        ConnectionState::Stopped
            .transition(ConnectionState::Idle, None)
            .is_none(),
        "Stopped→Idle should be invalid"
    );

    // Ready → Idle
    assert!(
        ConnectionState::Ready
            .transition(ConnectionState::Idle, None)
            .is_none(),
        "Ready→Idle should be invalid"
    );
}
