use super::*;
mod lifecycle;

#[test]
fn negotiated_limits_bound_data_and_include_variable_handle_overhead() {
    let config = Config::default();
    let limits = TransferLimits::new(
        &config,
        Limits {
            packet_len: Some(64),
            read_len: Some(100),
            write_len: Some(100),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(limits.read_len("handle").unwrap(), 51);
    assert_eq!(limits.write_len("handle").unwrap(), 33);
    assert!(limits.read_len(&"h".repeat(40)).is_err());
    assert!(limits.write_len(&"h".repeat(40)).is_err());
}

#[test]
fn tiny_read_write_limits_are_retained_without_rounding_up() {
    let limits = TransferLimits::new(
        &Config::default(),
        Limits {
            read_len: Some(1),
            write_len: Some(2),
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(limits.read_len("handle").unwrap(), 1);
    assert_eq!(limits.write_len("handle").unwrap(), 2);
}

#[test]
fn missing_zero_and_huge_limits_stay_bounded_by_client_defaults() {
    let config = Config::default();
    for value in [None, Some(0), Some(u64::MAX)] {
        let limits = TransferLimits::new(
            &config,
            Limits {
                packet_len: value,
                read_len: value,
                write_len: value,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(u64::from(limits.read_len("handle").unwrap()) <= CHUNK);
        assert!(limits.write_len("handle").unwrap() <= CHUNK as usize);
        assert!(limits.packet <= u64::from(config.max_packet_len));
        assert!(limits.write_packet <= u64::from(config.max_write_packet_len));
    }
}

#[test]
fn impossible_server_or_client_packet_limits_are_rejected() {
    let config = Config::default();
    for packet in [1, DATA_OVERHEAD, IO_OVERHEAD] {
        assert!(
            TransferLimits::new(
                &config,
                Limits {
                    packet_len: Some(packet),
                    ..Default::default()
                }
            )
            .is_err()
        );
    }
    let config = Config {
        max_packet_len: IO_OVERHEAD as u32,
        ..Config::default()
    };
    assert!(TransferLimits::new(&config, Limits::default()).is_err());
    let config = Config {
        max_write_packet_len: IO_OVERHEAD as u32,
        ..Config::default()
    };
    assert!(TransferLimits::new(&config, Limits::default()).is_err());
}
