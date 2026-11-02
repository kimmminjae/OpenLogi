use super::*;

fn repeat_press(id: u64, button: ButtonId, action: Action, now: Instant) -> ActivePress {
    ActivePress {
        token: PressToken::hook_for_test(id, button),
        behavior: PressBehavior::new(
            Some(&Binding::Single(action)),
            now,
            Some(ButtonRepeatRate::DEFAULT),
        ),
        target: ActionDispatchTarget::Keyboard,
    }
}

#[test]
fn hold_to_repeat_waits_for_the_initial_delay_and_stops_on_release() {
    let now = Instant::now();
    let press = repeat_press(1, ButtonId::Back, Action::VolumeUp, now);
    let mut state = ButtonState::default();
    let mut events = Vec::new();
    process_input(&mut state, ButtonInput::Down(press.clone()), &mut |event| {
        events.push(event);
    });
    assert_eq!(events.len(), 1);
    assert!(matches!(
        &events[0],
        ButtonRuntimeEvent::Started(press) if press.start_action() == Some(&Action::VolumeUp)
    ));

    emit_due_timers(&mut state, now + Duration::from_millis(399), &mut |event| {
        events.push(event);
    });
    assert!(triggered(&events).is_empty(), "a tap must not repeat");
    emit_due_timers(&mut state, now + Duration::from_millis(400), &mut |event| {
        events.push(event);
    });
    assert_eq!(triggered(&events), [&Action::VolumeUp]);

    process_input(
        &mut state,
        ButtonInput::Up {
            key: press.token.key.clone(),
            released_at: now + Duration::from_millis(401),
        },
        &mut |event| events.push(event),
    );
    emit_due_timers(&mut state, now + Duration::from_secs(5), &mut |event| {
        events.push(event);
    });
    assert_eq!(triggered(&events), [&Action::VolumeUp]);
    assert_eq!(state.next_deadline(), None);
}

#[test]
fn hold_to_repeat_uses_the_selected_steady_rate_without_early_fires() {
    let start = Instant::now();
    for (raw_rate, gap_ms) in [(2, 500), (20, 50), (50, 20)] {
        let mut schedule = RepeatSchedule::new(start, ButtonRepeatRate::try_new(raw_rate).unwrap());
        let mut now = start + Duration::from_millis(400);
        assert_eq!(schedule.deadline(), now);
        for _ in 0..100 {
            assert!(!schedule.fire_due(now.checked_sub(Duration::from_nanos(1)).unwrap()));
            assert!(schedule.fire_due(now));
            let next = now + Duration::from_millis(gap_ms);
            assert_eq!(schedule.deadline(), next);
            now = next;
        }
    }
}

#[test]
fn hold_to_repeat_slider_endpoints_produce_two_or_fifty_actions_per_second() {
    let now = Instant::now();
    for (rate, expected) in [(ButtonRepeatRate::MIN, 2), (ButtonRepeatRate::MAX, 50)] {
        let mut behavior =
            PressBehavior::new(Some(&Binding::Single(Action::VolumeUp)), now, Some(rate));
        let fired = (400..1400)
            .filter(|millis| {
                behavior
                    .fire_timer(now + Duration::from_millis(*millis))
                    .is_some()
            })
            .count();
        assert_eq!(fired, expected);
    }
}

#[test]
fn hold_to_repeat_never_catches_up_with_a_burst_after_a_stall() {
    let start = Instant::now();
    let mut schedule = RepeatSchedule::new(start, ButtonRepeatRate::DEFAULT);
    let resumed_at = start + Duration::from_secs(10);
    assert!(schedule.fire_due(resumed_at));
    assert!(!schedule.fire_due(resumed_at));
    assert_eq!(schedule.deadline(), resumed_at + Duration::from_millis(50));
}

#[test]
fn hold_to_repeat_keeps_every_quick_tap_and_pulse_single_step() {
    let now = Instant::now();
    for action in Action::catalog().into_iter().filter(Action::is_repeatable) {
        for pulse in [false, true] {
            let press = repeat_press(1, ButtonId::Back, action.clone(), now);
            let mut state = ButtonState::default();
            let mut events = Vec::new();
            let input = if pulse {
                ButtonInput::Pulse(press.clone())
            } else {
                ButtonInput::Down(press.clone())
            };
            process_input(&mut state, input, &mut |event| events.push(event));
            if !pulse {
                process_input(
                    &mut state,
                    ButtonInput::Up {
                        key: press.token.key.clone(),
                        released_at: now + Duration::from_millis(100),
                    },
                    &mut |event| events.push(event),
                );
            }
            emit_due_timers(&mut state, now + Duration::from_secs(1), &mut |event| {
                events.push(event);
            });
            assert!(matches!(
                events.as_slice(),
                [
                    ButtonRuntimeEvent::Started(started),
                    ButtonRuntimeEvent::Ended { reason: EndReason::Released, .. }
                ] if started.start_action() == Some(&action)
            ));
            assert_eq!(state.next_deadline(), None);
        }
    }
}

#[test]
fn hold_to_repeat_leaves_one_shots_gesture_clicks_and_long_actions_alone() {
    let now = Instant::now();
    let combo = "Ctrl+Space".parse().expect("valid shortcut");
    let mut bindings: Vec<_> = [
        Action::Copy,
        Action::PlayPause,
        Action::MuteVolume,
        Action::ToggleSmartShift,
        Action::HoldShortcut(combo),
    ]
    .into_iter()
    .map(Binding::Single)
    .collect();
    bindings.push(Binding::Gesture(std::collections::BTreeMap::from([(
        openlogi_core::binding::GestureDirection::Click,
        Action::VolumeUp,
    )])));
    bindings.push(wheel_hold(Action::VolumeUp));
    for binding in bindings {
        let behavior = PressBehavior::new(Some(&binding), now, Some(ButtonRepeatRate::DEFAULT));
        assert_eq!(behavior.deadline(), None, "{binding:?}");
    }

    let binding = long_press(Action::VolumeDown, Action::VolumeUp);
    let mut behavior = PressBehavior::new(Some(&binding), now, Some(ButtonRepeatRate::DEFAULT));
    assert_eq!(
        behavior.fire_timer(now + LONG_PRESS_THRESHOLD),
        Some(Action::VolumeUp)
    );
    assert_eq!(behavior.deadline(), None);
    assert_eq!(behavior.fire_timer(now + Duration::from_secs(5)), None);
}

#[test]
fn hold_to_repeat_tracks_each_button_and_device_independently() {
    let now = Instant::now();
    let first = repeat_press(1, ButtonId::Back, Action::VolumeUp, now);
    let second = repeat_press(
        2,
        ButtonId::Forward,
        Action::VolumeDown,
        now + Duration::from_millis(100),
    );
    let mut other_device = repeat_press(
        3,
        ButtonId::Back,
        Action::ScrollDown,
        now + Duration::from_millis(200),
    );
    other_device.token.key.source =
        ButtonSource::Hidpp(HidppSessionId::with_epoch("other-mouse", 7));
    let mut state = ButtonState::default();
    for press in [&first, &second, &other_device] {
        state.press(press.clone());
    }
    let mut events = Vec::new();
    emit_due_timers(&mut state, now + Duration::from_millis(400), &mut |event| {
        events.push(event);
    });
    assert_eq!(triggered(&events), [&Action::VolumeUp]);
    state.release(&first.token.key);
    emit_due_timers(&mut state, now + Duration::from_millis(500), &mut |event| {
        events.push(event);
    });
    state.cancel_source(&second.token.key.source);
    emit_due_timers(&mut state, now + Duration::from_millis(600), &mut |event| {
        events.push(event);
    });
    assert_eq!(
        triggered(&events),
        [&Action::VolumeUp, &Action::VolumeDown, &Action::ScrollDown]
    );
    emit_due_timers(&mut state, now + Duration::from_millis(820), &mut |event| {
        events.push(event);
    });
    assert_eq!(triggered(&events).last(), Some(&&Action::ScrollDown));
    assert_eq!(triggered(&events).len(), 4);
}

#[test]
fn hold_to_repeat_disabled_keeps_volume_and_scroll_single_step_for_hook_and_hidpp() {
    let (commands, queued) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
    let input = ButtonInputHandle {
        events: commands,
        generation: Arc::new(AtomicU64::new(0)),
        accepting: Arc::new(AtomicBool::new(true)),
        next_press: Arc::new(AtomicU64::new(1)),
        repeat_rate: Arc::new(AtomicU8::new(u8::from(ButtonRepeatRate::DEFAULT))),
    };
    let settings = AppSettings {
        hold_to_repeat: false,
        ..AppSettings::default()
    };
    input.configure_repeat(&settings);
    assert_eq!(input.repeat_rate(), None);
    assert!(matches!(queued.recv().unwrap(), ButtonCommand::Wake));
    let session = HidppSessionId::with_epoch("test-mouse", 99);
    for hidpp in [false, true] {
        for action in Action::catalog().into_iter().filter(Action::is_repeatable) {
            let binding = Binding::Single(action.clone());
            let token = if hidpp {
                input.try_hidpp_down(
                    &session,
                    ButtonId::Back,
                    Some(&binding),
                    ActionDispatchTarget::Keyboard,
                )
            } else {
                input.try_hook_down(ButtonId::Back, Some(&binding))
            };
            assert!(token.is_some());
            let ButtonCommand::Input {
                input: ButtonInput::Down(press),
                ..
            } = queued.recv().unwrap()
            else {
                panic!("expected queued down edge");
            };
            let mut state = ButtonState::default();
            let mut events = Vec::new();
            process_input(&mut state, ButtonInput::Down(press), &mut |event| {
                events.push(event);
            });
            emit_due_timers(
                &mut state,
                Instant::now() + Duration::from_secs(10),
                &mut |event| events.push(event),
            );
            assert_eq!(events.len(), 1);
            assert!(
                matches!(&events[0], ButtonRuntimeEvent::Started(press) if press.start_action() == Some(&action))
            );
            assert_eq!(state.next_deadline(), None);
        }
    }
}

#[test]
fn hold_to_repeat_settings_changes_invalidate_old_presses_and_new_presses_use_new_rate() {
    let (commands, queued) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
    let input = ButtonInputHandle {
        events: commands,
        generation: Arc::new(AtomicU64::new(0)),
        accepting: Arc::new(AtomicBool::new(true)),
        next_press: Arc::new(AtomicU64::new(1)),
        repeat_rate: Arc::new(AtomicU8::new(u8::from(ButtonRepeatRate::DEFAULT))),
    };
    let binding = Binding::Single(Action::VolumeDown);
    let old = input.try_hook_down(ButtonId::Back, Some(&binding)).unwrap();
    let mut settings = AppSettings {
        button_repeat_rate: ButtonRepeatRate::MAX,
        ..AppSettings::default()
    };
    input.configure_repeat(&settings);
    assert!(!input.try_trigger_while_pressed(&old, &Action::VolumeDown));
    let generation = input.generation.load(Ordering::Acquire);
    input.configure_repeat(&settings);
    assert_eq!(
        input.generation.load(Ordering::Acquire),
        generation,
        "unchanged settings must not interrupt a hold"
    );
    let new = input.try_hook_down(ButtonId::Back, Some(&binding)).unwrap();
    assert_ne!(old.generation, new.generation);
    let ButtonCommand::Input {
        input: ButtonInput::Down(_),
        ..
    } = queued.recv().unwrap()
    else {
        panic!("old down");
    };
    assert!(matches!(queued.recv().unwrap(), ButtonCommand::Wake));
    let ButtonCommand::Input {
        input: ButtonInput::Down(mut press),
        ..
    } = queued.recv().unwrap()
    else {
        panic!("new down");
    };
    let due = press.behavior.deadline().unwrap();
    assert_eq!(press.behavior.fire_timer(due), Some(Action::VolumeDown));
    assert_eq!(
        press.behavior.deadline(),
        Some(due + Duration::from_millis(20))
    );
    settings.hold_to_repeat = false;
    input.configure_repeat(&settings);
    assert!(!input.try_trigger_while_pressed(&new, &Action::VolumeDown));
}

#[test]
fn hold_to_repeat_repress_restarts_the_delay_and_rejects_the_stale_timer() {
    let now = Instant::now();
    let old = repeat_press(1, ButtonId::Back, Action::VolumeUp, now);
    let new = repeat_press(
        2,
        ButtonId::Back,
        Action::VolumeDown,
        now + Duration::from_millis(350),
    );
    let mut state = ButtonState::default();
    state.press(old.clone());
    let mut events = Vec::new();
    process_input(&mut state, ButtonInput::Down(new.clone()), &mut |event| {
        events.push(event);
    });
    emit_selected_timers(
        &mut state,
        &[old.token],
        now + Duration::from_millis(750),
        &mut |event| events.push(event),
    );
    assert!(triggered(&events).is_empty());
    emit_due_timers(&mut state, now + Duration::from_millis(749), &mut |event| {
        events.push(event);
    });
    assert!(triggered(&events).is_empty());
    emit_due_timers(&mut state, now + Duration::from_millis(750), &mut |event| {
        events.push(event);
    });
    assert_eq!(triggered(&events), [&Action::VolumeDown]);
    assert!(matches!(
        &events[0],
        ButtonRuntimeEvent::Ended {
            reason: EndReason::Canceled(CancelReason::RepeatedDown),
            ..
        }
    ));
}

#[test]
fn hold_to_repeat_queued_release_or_cancellation_beats_an_overdue_timer() {
    let now = Instant::now();
    let mut press = repeat_press(
        1,
        ButtonId::Back,
        Action::VolumeUp,
        now.checked_sub(Duration::from_secs(1))
            .expect("test process should have run for a second"),
    );
    press.target = ActionDispatchTarget::Pointer {
        target: openlogi_hook::PointerTarget::Unavailable,
        fallback_safari_pid: None,
    };
    for command in [
        ButtonCommand::Input {
            generation: 0,
            input: up(&press),
        },
        ButtonCommand::CancelSource(press.token.key.source.clone()),
        ButtonCommand::CancelHooks,
        ButtonCommand::CancelStalePress(press.token.clone()),
        ButtonCommand::CancelPointerExcept(openlogi_hook::PointerTarget::Desktop),
    ] {
        let (commands, queued) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let (_shutdown, shutdown) = mpsc::channel();
        commands.send(command).expect("queue accepts cancellation");
        let mut state = ButtonState::default();
        state.press(press.clone());
        let mut events = Vec::new();
        assert!(!settle_due_timers(
            &queued,
            &shutdown,
            &AtomicU64::new(0),
            &mut 0,
            &mut state,
            None,
            &mut |event| events.push(event),
        ));
        assert!(triggered(&events).is_empty());
        assert!(matches!(
            events.as_slice(),
            [ButtonRuntimeEvent::Ended { .. }]
        ));
        assert_eq!(state.next_deadline(), None);
    }
}

#[test]
fn hold_to_repeat_invalidation_and_queue_overflow_cancel_before_an_overdue_timer() {
    let now = Instant::now();
    let pressed_at = now
        .checked_sub(Duration::from_secs(1))
        .expect("test process should have run for a second");
    for overflow in [false, true] {
        let (commands, queued) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
        let generation = Arc::new(AtomicU64::new(0));
        let input = ButtonInputHandle {
            events: commands,
            generation: Arc::clone(&generation),
            accepting: Arc::new(AtomicBool::new(true)),
            next_press: Arc::new(AtomicU64::new(2)),
            repeat_rate: Arc::new(AtomicU8::new(u8::from(ButtonRepeatRate::DEFAULT))),
        };
        let mut state = ButtonState::default();
        state.press(repeat_press(
            1,
            ButtonId::Back,
            Action::VolumeUp,
            pressed_at,
        ));
        if overflow {
            for _ in 0..EVENT_QUEUE_CAPACITY {
                input
                    .events
                    .send(ButtonCommand::Wake)
                    .expect("queue accepts its capacity");
            }
            assert!(!input.try_hook_up(ButtonId::Back));
        } else {
            input.invalidate_all();
        }
        let (_shutdown, shutdown) = mpsc::channel();
        let mut events = Vec::new();
        assert!(!settle_due_timers(
            &queued,
            &shutdown,
            &generation,
            &mut 0,
            &mut state,
            None,
            &mut |event| events.push(event),
        ));
        assert!(matches!(
            events.as_slice(),
            [ButtonRuntimeEvent::Ended {
                reason: EndReason::Canceled(CancelReason::Invalidated),
                ..
            }]
        ));
        assert_eq!(state.next_deadline(), None);
    }
}

#[test]
fn hold_to_repeat_shutdown_cancels_before_an_overdue_timer() {
    let now = Instant::now();
    let pressed_at = now
        .checked_sub(Duration::from_secs(1))
        .expect("test process should have run for a second");
    let mut state = ButtonState::default();
    state.press(repeat_press(
        1,
        ButtonId::Back,
        Action::VolumeUp,
        pressed_at,
    ));
    let (_commands, queued) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
    let (shutdown, shutdown_rx) = mpsc::channel();
    let (done, wait) = mpsc::sync_channel(1);
    shutdown
        .send(ShutdownRequest { done })
        .expect("worker accepts shutdown");
    let mut events = Vec::new();
    assert!(settle_due_timers(
        &queued,
        &shutdown_rx,
        &AtomicU64::new(0),
        &mut 0,
        &mut state,
        None,
        &mut |event| events.push(event),
    ));
    wait.try_recv().expect("shutdown was acknowledged");
    assert!(matches!(
        events.as_slice(),
        [ButtonRuntimeEvent::Ended {
            reason: EndReason::Canceled(CancelReason::Shutdown),
            ..
        }]
    ));
    assert_eq!(state.next_deadline(), None);
}

#[test]
fn hold_to_repeat_deadlines_precede_unrelated_queued_actions() {
    let now = Instant::now();
    let pressed_at = now
        .checked_sub(Duration::from_secs(1))
        .expect("test process should have run for a second");
    let mut state = ButtonState::default();
    state.press(repeat_press(
        1,
        ButtonId::Back,
        Action::VolumeUp,
        pressed_at,
    ));
    let (commands, queued) = mpsc::sync_channel(EVENT_QUEUE_CAPACITY);
    commands
        .send(ButtonCommand::Input {
            generation: 0,
            input: ButtonInput::Pulse(hook_press(2, ButtonId::Forward)),
        })
        .expect("queue accepts unrelated action");
    let (_shutdown, shutdown) = mpsc::channel();
    let mut events = Vec::new();
    assert!(!settle_due_timers(
        &queued,
        &shutdown,
        &AtomicU64::new(0),
        &mut 0,
        &mut state,
        None,
        &mut |event| events.push(event),
    ));
    assert!(matches!(
        events.as_slice(),
        [
            ButtonRuntimeEvent::Triggered {
                action: Action::VolumeUp,
                ..
            },
            ButtonRuntimeEvent::Started(_),
            ButtonRuntimeEvent::Ended { .. }
        ]
    ));
}

#[test]
fn hold_to_repeat_an_idle_worker_repeats_hidpp_until_release() {
    let (sent, received) = mpsc::channel();
    let mut owner = ButtonRuntimeOwner::spawn(move |event| {
        sent.send(event).expect("test receiver stays connected");
    })
    .expect("button worker starts");
    let input = owner.input();
    let session = HidppSessionId::with_epoch("mouse", 7);
    let binding = Binding::Single(Action::VolumeUp);
    let token = input
        .try_hidpp_down(
            &session,
            ButtonId::Back,
            Some(&binding),
            ActionDispatchTarget::Keyboard,
        )
        .expect("down is accepted");
    assert!(matches!(
        recv_event(&received),
        ButtonRuntimeEvent::Started(_)
    ));
    assert!(matches!(
        recv_event(&received),
        ButtonRuntimeEvent::Triggered { press, action: Action::VolumeUp }
            if press.token == token
    ));
    assert!(input.try_hidpp_up(&session, ButtonId::Back));
    loop {
        match recv_event(&received) {
            ButtonRuntimeEvent::Ended {
                press,
                reason: EndReason::Released,
            } => {
                assert_eq!(press.token, token);
                break;
            }
            ButtonRuntimeEvent::Triggered { press, .. } => assert_eq!(press.token, token),
            _ => panic!("a held press must repeat or end"),
        }
    }
    assert!(owner.shutdown());
    assert_eq!(received.try_iter().count(), 0);
}
