use super::*;

pub(in crate::web) fn runtime_cloud_rollouts_state_path(
    bundle_root: Option<&PathBuf>,
) -> Option<PathBuf> {
    let root = bundle_root?;
    Some(
        root.join(".trust")
            .join("runtime-cloud")
            .join("rollouts-state.json"),
    )
}

pub(in crate::web) fn runtime_cloud_rollouts_load_state(
    path: Option<&Path>,
) -> RuntimeCloudRolloutManagerState {
    let Some(path) = path else {
        return RuntimeCloudRolloutManagerState {
            next_id: 1,
            rollouts: BTreeMap::new(),
        };
    };
    let Ok(text) = std::fs::read_to_string(path) else {
        return RuntimeCloudRolloutManagerState {
            next_id: 1,
            rollouts: BTreeMap::new(),
        };
    };
    serde_json::from_str::<RuntimeCloudRolloutManagerState>(&text).unwrap_or(
        RuntimeCloudRolloutManagerState {
            next_id: 1,
            rollouts: BTreeMap::new(),
        },
    )
}

pub(in crate::web) fn runtime_cloud_rollouts_store_state(
    path: Option<&Path>,
    state: &RuntimeCloudRolloutManagerState,
) {
    let Some(path) = path else {
        return;
    };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(state) {
        let _ = std::fs::write(path, text);
    }
}

pub(in crate::web) fn runtime_cloud_rollouts_snapshot(
    rollouts: &Mutex<RuntimeCloudRolloutManagerState>,
) -> Vec<RuntimeCloudRolloutRecord> {
    rollouts
        .lock()
        .map(|guard| guard.rollouts.values().cloned().collect::<Vec<_>>())
        .unwrap_or_default()
}

pub(in crate::web) fn runtime_cloud_rollout_create(
    rollouts: &Mutex<RuntimeCloudRolloutManagerState>,
    config: &Mutex<RuntimeCloudConfigAgentState>,
    payload: &RuntimeCloudRolloutCreateRequest,
    persist_path: Option<&Path>,
) -> Result<RuntimeCloudRolloutRecord, (ReasonCode, String)> {
    match evaluate_compatibility(payload.api_version.as_str(), RUNTIME_CLOUD_API_VERSION) {
        Ok(ContractCompatibility::Exact | ContractCompatibility::AdditiveWithinMajor) => {}
        Ok(ContractCompatibility::BreakingMajor) => {
            return Err((
                ReasonCode::ContractViolation,
                format!(
                    "unsupported api_version '{}' for runtime cloud {}",
                    payload.api_version, RUNTIME_CLOUD_API_VERSION
                ),
            ));
        }
        Err(error) => {
            return Err((ReasonCode::ContractViolation, error.to_string()));
        }
    }
    if payload.actor.trim().is_empty() {
        return Err((
            ReasonCode::ContractViolation,
            "actor must not be empty".to_string(),
        ));
    }
    if payload.target_runtimes.is_empty() {
        return Err((
            ReasonCode::ContractViolation,
            "target_runtimes must include at least one runtime".to_string(),
        ));
    }

    let config_snapshot = config
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_else(|_| runtime_cloud_config_initial_state());
    let desired_revision = payload
        .desired_revision
        .unwrap_or(config_snapshot.meta.desired_revision);
    if desired_revision > config_snapshot.meta.desired_revision {
        return Err((
            ReasonCode::RevisionConflict,
            format!(
                "desired_revision {} is newer than current desired_revision {}",
                desired_revision, config_snapshot.meta.desired_revision
            ),
        ));
    }

    let now = now_ns();
    let mut guard = rollouts.lock().map_err(|_| {
        (
            ReasonCode::TransportFailure,
            "rollout state unavailable".to_string(),
        )
    })?;
    let rollout_id = format!("rollout-{}", guard.next_id);
    guard.next_id = guard.next_id.saturating_add(1);

    let mut seen = std::collections::BTreeSet::new();
    let mut targets = Vec::new();
    for runtime_id in &payload.target_runtimes {
        if !seen.insert(runtime_id.as_str()) {
            continue;
        }
        targets.push(RuntimeCloudRolloutTargetRecord {
            runtime_id: runtime_id.clone(),
            state: RuntimeCloudRolloutTargetState::Queued,
            verification: None,
            blocked_reason: None,
            error: None,
            updated_at_ns: now,
        });
    }
    if targets.is_empty() {
        return Err((
            ReasonCode::ContractViolation,
            "target_runtimes must include at least one unique runtime".to_string(),
        ));
    }

    let rollout = RuntimeCloudRolloutRecord {
        api_version: RUNTIME_CLOUD_API_VERSION.to_string(),
        rollout_id: rollout_id.clone(),
        actor: payload.actor.clone(),
        desired_revision,
        state: RuntimeCloudRolloutState::Queued,
        paused: false,
        created_at_ns: now,
        updated_at_ns: now,
        targets,
    };
    guard.rollouts.insert(rollout_id, rollout.clone());
    runtime_cloud_rollouts_store_state(persist_path, &guard);
    Ok(rollout)
}

pub(in crate::web) fn runtime_cloud_rollout_apply_action(
    rollouts: &Mutex<RuntimeCloudRolloutManagerState>,
    rollout_id: &str,
    action: &str,
    persist_path: Option<&Path>,
) -> RuntimeCloudRolloutActionResponse {
    let mut guard = match rollouts.lock() {
        Ok(guard) => guard,
        Err(_) => {
            return RuntimeCloudRolloutActionResponse {
                ok: false,
                action: action.to_string(),
                denial_code: Some(ReasonCode::TransportFailure),
                error: Some("rollout state unavailable".to_string()),
                rollout: None,
            };
        }
    };
    let Some(rollout) = guard.rollouts.get_mut(rollout_id) else {
        return RuntimeCloudRolloutActionResponse {
            ok: false,
            action: action.to_string(),
            denial_code: Some(ReasonCode::PeerNotAvailable),
            error: Some(format!("unknown rollout_id '{rollout_id}'")),
            rollout: None,
        };
    };

    let now = now_ns();
    let response = match action {
        "pause" => {
            if runtime_cloud_rollout_is_terminal(rollout.state) {
                RuntimeCloudRolloutActionResponse {
                    ok: false,
                    action: "pause".to_string(),
                    denial_code: Some(ReasonCode::Conflict),
                    error: Some("cannot pause terminal rollout".to_string()),
                    rollout: Some(rollout.clone()),
                }
            } else if rollout.paused {
                RuntimeCloudRolloutActionResponse {
                    ok: true,
                    action: "noop".to_string(),
                    denial_code: None,
                    error: None,
                    rollout: Some(rollout.clone()),
                }
            } else {
                rollout.paused = true;
                rollout.updated_at_ns = now;
                RuntimeCloudRolloutActionResponse {
                    ok: true,
                    action: "paused".to_string(),
                    denial_code: None,
                    error: None,
                    rollout: Some(rollout.clone()),
                }
            }
        }
        "resume" => {
            if runtime_cloud_rollout_is_terminal(rollout.state) {
                RuntimeCloudRolloutActionResponse {
                    ok: false,
                    action: "resume".to_string(),
                    denial_code: Some(ReasonCode::Conflict),
                    error: Some("cannot resume terminal rollout".to_string()),
                    rollout: Some(rollout.clone()),
                }
            } else if !rollout.paused {
                RuntimeCloudRolloutActionResponse {
                    ok: true,
                    action: "noop".to_string(),
                    denial_code: None,
                    error: None,
                    rollout: Some(rollout.clone()),
                }
            } else {
                rollout.paused = false;
                rollout.updated_at_ns = now;
                RuntimeCloudRolloutActionResponse {
                    ok: true,
                    action: "resumed".to_string(),
                    denial_code: None,
                    error: None,
                    rollout: Some(rollout.clone()),
                }
            }
        }
        "abort" => {
            if runtime_cloud_rollout_is_terminal(rollout.state) {
                RuntimeCloudRolloutActionResponse {
                    ok: false,
                    action: "abort".to_string(),
                    denial_code: Some(ReasonCode::Conflict),
                    error: Some("cannot abort terminal rollout".to_string()),
                    rollout: Some(rollout.clone()),
                }
            } else {
                rollout.paused = false;
                rollout.state = RuntimeCloudRolloutState::Aborted;
                rollout.updated_at_ns = now;
                for target in &mut rollout.targets {
                    target.state = RuntimeCloudRolloutTargetState::Aborted;
                    target.error = Some("operator aborted rollout".to_string());
                    target.updated_at_ns = now;
                }
                RuntimeCloudRolloutActionResponse {
                    ok: true,
                    action: "aborted".to_string(),
                    denial_code: None,
                    error: None,
                    rollout: Some(rollout.clone()),
                }
            }
        }
        _ => RuntimeCloudRolloutActionResponse {
            ok: false,
            action: action.to_string(),
            denial_code: Some(ReasonCode::ContractViolation),
            error: Some(format!("unsupported rollout action '{action}'")),
            rollout: Some(rollout.clone()),
        },
    };
    runtime_cloud_rollouts_store_state(persist_path, &guard);
    response
}

pub(in crate::web) fn runtime_cloud_rollouts_reconcile_once(
    rollouts: &Mutex<RuntimeCloudRolloutManagerState>,
    config: &Mutex<RuntimeCloudConfigAgentState>,
    persist_path: Option<&Path>,
) {
    let config_snapshot = config
        .lock()
        .map(|guard| guard.clone())
        .unwrap_or_else(|_| runtime_cloud_config_initial_state());
    let mut guard = match rollouts.lock() {
        Ok(guard) => guard,
        Err(_) => return,
    };
    let mut changed = false;
    for rollout in guard.rollouts.values_mut() {
        if rollout.paused || runtime_cloud_rollout_is_terminal(rollout.state) {
            continue;
        }
        if runtime_cloud_rollout_advance(rollout, &config_snapshot) {
            changed = true;
        }
    }
    if changed {
        runtime_cloud_rollouts_store_state(persist_path, &guard);
    }
}

pub(in crate::web) fn runtime_cloud_rollout_advance(
    rollout: &mut RuntimeCloudRolloutRecord,
    config: &RuntimeCloudConfigAgentState,
) -> bool {
    let now = now_ns();
    let before = rollout.state;
    let next = match rollout.state {
        RuntimeCloudRolloutState::Queued => Some(RuntimeCloudRolloutState::Staging),
        RuntimeCloudRolloutState::Staging => Some(RuntimeCloudRolloutState::Staged),
        RuntimeCloudRolloutState::Staged => Some(RuntimeCloudRolloutState::Applying),
        RuntimeCloudRolloutState::Applying => {
            let elapsed = now.saturating_sub(rollout.updated_at_ns);
            if elapsed >= RUNTIME_CLOUD_ROLLOUT_APPLY_TIMEOUT_NS {
                rollout.state = RuntimeCloudRolloutState::Failed;
                rollout.updated_at_ns = now;
                for target in &mut rollout.targets {
                    target.state = RuntimeCloudRolloutTargetState::Failed;
                    target.blocked_reason = Some(ReasonCode::Timeout);
                    target.error = Some("rollout applying timed out".to_string());
                    target.updated_at_ns = now;
                }
                return true;
            }
            if matches!(
                config.status.state,
                RuntimeCloudConfigState::Blocked | RuntimeCloudConfigState::Error
            ) {
                rollout.state = RuntimeCloudRolloutState::Failed;
                rollout.updated_at_ns = now;
                for target in &mut rollout.targets {
                    target.state = RuntimeCloudRolloutTargetState::Failed;
                    target.blocked_reason = config.status.blocked_reason;
                    target.error = config.status.errors.first().cloned();
                    target.updated_at_ns = now;
                }
                return true;
            }
            if config.meta.reported_revision >= rollout.desired_revision {
                Some(RuntimeCloudRolloutState::Applied)
            } else {
                None
            }
        }
        RuntimeCloudRolloutState::Applied => Some(RuntimeCloudRolloutState::Verifying),
        RuntimeCloudRolloutState::Verifying => {
            if matches!(
                config.status.state,
                RuntimeCloudConfigState::Blocked | RuntimeCloudConfigState::Error
            ) {
                rollout.state = RuntimeCloudRolloutState::Failed;
                rollout.updated_at_ns = now;
                for target in &mut rollout.targets {
                    target.state = RuntimeCloudRolloutTargetState::Failed;
                    target.blocked_reason = config.status.blocked_reason;
                    target.error = config.status.errors.first().cloned();
                    target.updated_at_ns = now;
                }
                return true;
            }
            if config.meta.reported_revision >= rollout.desired_revision
                && config.status.state == RuntimeCloudConfigState::InSync
            {
                Some(RuntimeCloudRolloutState::Verified)
            } else {
                None
            }
        }
        RuntimeCloudRolloutState::Verified => Some(RuntimeCloudRolloutState::Completed),
        RuntimeCloudRolloutState::Completed
        | RuntimeCloudRolloutState::Failed
        | RuntimeCloudRolloutState::Aborted => None,
    };

    let Some(next) = next else {
        return false;
    };
    rollout.state = next;
    rollout.updated_at_ns = now;
    for target in &mut rollout.targets {
        target.state = match next {
            RuntimeCloudRolloutState::Queued => RuntimeCloudRolloutTargetState::Queued,
            RuntimeCloudRolloutState::Staging => RuntimeCloudRolloutTargetState::Staging,
            RuntimeCloudRolloutState::Staged => RuntimeCloudRolloutTargetState::Staged,
            RuntimeCloudRolloutState::Applying => RuntimeCloudRolloutTargetState::Applying,
            RuntimeCloudRolloutState::Applied => RuntimeCloudRolloutTargetState::Applied,
            RuntimeCloudRolloutState::Verifying => RuntimeCloudRolloutTargetState::Verifying,
            RuntimeCloudRolloutState::Verified => {
                target.verification = Some(format!(
                    "reported_revision={} status=in_sync",
                    config.meta.reported_revision
                ));
                RuntimeCloudRolloutTargetState::Verified
            }
            RuntimeCloudRolloutState::Completed => target.state,
            RuntimeCloudRolloutState::Failed => RuntimeCloudRolloutTargetState::Failed,
            RuntimeCloudRolloutState::Aborted => RuntimeCloudRolloutTargetState::Aborted,
        };
        target.updated_at_ns = now;
    }
    before != rollout.state
}

pub(in crate::web) fn runtime_cloud_rollout_is_terminal(state: RuntimeCloudRolloutState) -> bool {
    matches!(
        state,
        RuntimeCloudRolloutState::Completed
            | RuntimeCloudRolloutState::Failed
            | RuntimeCloudRolloutState::Aborted
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_cloud_rollout_applying_timeout_transitions_to_failed() {
        let mut rollout = RuntimeCloudRolloutRecord {
            api_version: RUNTIME_CLOUD_API_VERSION.to_string(),
            rollout_id: "rollout-timeout-1".to_string(),
            actor: "spiffe://trust/default-site/operator-1".to_string(),
            desired_revision: 5,
            state: RuntimeCloudRolloutState::Applying,
            paused: false,
            created_at_ns: now_ns(),
            updated_at_ns: now_ns().saturating_sub(RUNTIME_CLOUD_ROLLOUT_APPLY_TIMEOUT_NS + 1),
            targets: vec![RuntimeCloudRolloutTargetRecord {
                runtime_id: "runtime-a".to_string(),
                state: RuntimeCloudRolloutTargetState::Applying,
                verification: None,
                blocked_reason: None,
                error: None,
                updated_at_ns: now_ns(),
            }],
        };
        let config = runtime_cloud_config_initial_state();

        let changed = runtime_cloud_rollout_advance(&mut rollout, &config);
        assert!(changed);
        assert_eq!(rollout.state, RuntimeCloudRolloutState::Failed);
        assert_eq!(
            rollout.targets[0].state,
            RuntimeCloudRolloutTargetState::Failed
        );
        assert_eq!(rollout.targets[0].blocked_reason, Some(ReasonCode::Timeout));
    }
}
