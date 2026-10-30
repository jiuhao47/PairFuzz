use libafl::{
    executors::{forkserver::ForkserverExecutor, StdChildArgs},
    inputs::Input,
    mutators::Tokens,
    observers::{MapObserver, Observer, ObserversTuple},
    state::HasExecutions,
};
use libafl_bolts::{
    shmem::{StdShMem, StdShMemProvider},
    tuples::{tuple_list, Prepend},
    StdTargetArgs, Truncate,
};
use std::{path::Path, time::Duration};

use crate::{
    observers::ReturnValueObserver,
    utils::logging::{error, info, warn},
};

#[derive(Clone, Copy)]
pub(crate) enum TargetRole {
    A,
    B,
}

pub(crate) struct TargetNames {
    pub(crate) label: &'static str,
    pub(crate) return_observer: &'static str,
    pub(crate) rv_env: &'static str,
    pub(crate) rv_size_env: &'static str,
    pub(crate) rv_temp_env: &'static str,
    pub(crate) coverage_temp_env: &'static str,
}

pub(crate) const fn names(role: TargetRole) -> TargetNames {
    match role {
        TargetRole::A => TargetNames {
            label: "A",
            return_observer: "first_return",
            rv_env: "__PAIRFUZZ_RV_SHM_ID_A",
            rv_size_env: "__PAIRFUZZ_RV_SHM_ID_A_SIZE",
            rv_temp_env: "__PAIRFUZZ_TMP_RV_A",
            coverage_temp_env: "__PAIRFUZZ_TMP_COV_A",
        },
        TargetRole::B => TargetNames {
            label: "B",
            return_observer: "second_return",
            rv_env: "__PAIRFUZZ_RV_SHM_ID_B",
            rv_size_env: "__PAIRFUZZ_RV_SHM_ID_B_SIZE",
            rv_temp_env: "__PAIRFUZZ_TMP_RV_B",
            coverage_temp_env: "__PAIRFUZZ_TMP_COV_B",
        },
    }
}

pub(crate) struct TargetSpec<'a> {
    pub(crate) role: TargetRole,
    pub(crate) program: Option<&'a Path>,
    pub(crate) coverage_id: Option<&'a str>,
    pub(crate) coverage_shmem: Option<&'a StdShMem>,
    pub(crate) return_shmem: Option<StdShMem>,
    pub(crate) persistent_requested: bool,
    pub(crate) verbose: u8,
    pub(crate) timeout: Duration,
    pub(crate) num_edges: usize,
}

pub(crate) fn build_executor<I, S, A, MO>(
    spec: TargetSpec<'_>,
    map_observer: A,
    mut rv_observer: ReturnValueObserver,
    provider: &mut StdShMemProvider,
    tokens: &mut Tokens,
) -> ForkserverExecutor<I, (A, (ReturnValueObserver, ())), S, StdShMem>
where
    I: Input,
    S: HasExecutions,
    A: Observer<I, S> + AsMut<MO>,
    MO: MapObserver + Truncate,
    (ReturnValueObserver, ()): ObserversTuple<I, S> + Prepend<MO>,
{
    let target_names = names(spec.role);
    let Some(program) = spec.program else {
        error(
            "main",
            &format!(
                "prog-{} not provided; forkserver executor {} cannot be created",
                target_names.label.to_ascii_lowercase(),
                target_names.label
            ),
        );
        std::process::exit(1);
    };

    let capabilities = match crate::driver::bin_check::check_binary(
        &mut program.to_path_buf(),
        false,
        "__AFL_SHM_ID",
    ) {
        Ok(capabilities) => capabilities,
        Err(err) => {
            error(
                "main",
                &format!(
                    "Target {} binary validation failed: {}",
                    target_names.label, err
                ),
            );
            std::process::exit(1);
        }
    };
    info(
        "main",
        &format!(
            "Detected capabilities for {}: persistent={} defer={} asan={} lsan={} instrumented={}",
            target_names.label,
            capabilities.persistent,
            capabilities.defer,
            capabilities.has_asan,
            capabilities.has_lsan,
            capabilities.instrumented
        ),
    );

    // A legacy publisher cannot signal presence; reject it before any input runs.
    const RESULT_PROTOCOL_MARKER: &[u8] = b"__PAIRFUZZ_RV_PROTOCOL\0";
    let supports_result_protocol = std::fs::read(program)
        .map(|data| {
            data.windows(RESULT_PROTOCOL_MARKER.len())
                .any(|w| w == RESULT_PROTOCOL_MARKER)
        })
        .unwrap_or(false);
    if !supports_result_protocol {
        error("main", &format!("Target {} lacks semantic result protocol 2; rebuild it with the current PairFuzz compiler runtime", target_names.label));
        std::process::exit(1);
    }

    let mut builder = ForkserverExecutor::builder()
        .program(program.as_os_str().to_os_string())
        .shmem_provider(provider)
        .autotokens(tokens)
        .is_persistent(spec.persistent_requested && capabilities.persistent)
        .debug_child(spec.verbose >= 3)
        .coverage_map_size(spec.num_edges)
        .timeout(spec.timeout);

    let mut child_envs: Vec<(&str, String)> = Vec::new();
    if let Some(id) = spec.coverage_id {
        child_envs.push(("__AFL_SHM_ID", id.to_owned()));
        info(
            "main",
            &format!("Injected __AFL_SHM_ID for {} = {}", target_names.label, id),
        );
    }

    if let Some(return_shmem) = spec.return_shmem {
        match crate::observers::shmem::rv_shmem::extract_id_size_and_clear(
            &return_shmem,
            target_names.rv_temp_env,
        ) {
            Ok((id, size)) => {
                child_envs.push((target_names.rv_env, id.clone()));
                child_envs.push((target_names.rv_size_env, size.clone()));
                child_envs.push(("__PAIRFUZZ_RV_ROLE", target_names.label.to_owned()));
                child_envs.push(("__PAIRFUZZ_RV_PROTOCOL", "2".to_owned()));
                info(
                    "main",
                    &format!(
                        "Prepared {} for builder (id={} size={})",
                        target_names.rv_env, id, size
                    ),
                );
            }
            Err(err) => warn(
                "main",
                &format!(
                    "failed to extract rv shmem id {}: {}",
                    target_names.label, err
                ),
            ),
        }
        if let Err(err) = rv_observer.set_shmem(return_shmem) {
            error(
                "main",
                &format!(
                    "failed to attach semantic result mapping for {}: {}",
                    target_names.label, err
                ),
            );
            std::process::exit(1);
        }
        info(
            "main",
            &format!(
                "Attached rv shmem to executor observer {} ({})",
                target_names.label, target_names.return_observer
            ),
        );
    }

    if let Some(coverage_shmem) = spec.coverage_shmem {
        if let Err(err) =
            crate::observers::shmem::rv_shmem::write_shmem_to_env(coverage_shmem, "__AFL_SHM_ID")
        {
            warn(
                "main",
                &format!(
                    "failed to write __AFL_SHM_ID for {}: {}",
                    target_names.label, err
                ),
            );
        }
        info(
            "main",
            &format!(
                "Wrote __AFL_SHM_ID into parent env for executor {}",
                target_names.label
            ),
        );
    }

    for (key, value) in child_envs.drain(..) {
        builder = builder.env(key, value);
    }

    for key in &[target_names.rv_env, target_names.rv_size_env] {
        std::env::remove_var(key);
    }

    match builder.build_dynamic_map(map_observer, tuple_list!(rv_observer)) {
        Ok(executor) => executor,
        Err(err) => {
            error(
                "main",
                &format!(
                    "Failed to create forkserver executor {}: {}",
                    target_names.label, err
                ),
            );
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{names, TargetRole};

    #[test]
    fn target_role_a_uses_only_a_names() {
        let value = names(TargetRole::A);
        assert_eq!(value.label, "A");
        assert_eq!(value.return_observer, "first_return");
        assert_eq!(value.rv_env, "__PAIRFUZZ_RV_SHM_ID_A");
        assert_eq!(value.rv_size_env, "__PAIRFUZZ_RV_SHM_ID_A_SIZE");
        assert_eq!(value.rv_temp_env, "__PAIRFUZZ_TMP_RV_A");
        assert_eq!(value.coverage_temp_env, "__PAIRFUZZ_TMP_COV_A");
    }

    #[test]
    fn target_role_b_uses_only_b_names() {
        let value = names(TargetRole::B);
        assert_eq!(value.label, "B");
        assert_eq!(value.return_observer, "second_return");
        assert_eq!(value.rv_env, "__PAIRFUZZ_RV_SHM_ID_B");
        assert_eq!(value.rv_size_env, "__PAIRFUZZ_RV_SHM_ID_B_SIZE");
        assert_eq!(value.rv_temp_env, "__PAIRFUZZ_TMP_RV_B");
        assert_eq!(value.coverage_temp_env, "__PAIRFUZZ_TMP_COV_B");
    }
}

/// Build a target-local coverage observer. The caller keeps the mapping alive.
pub(crate) unsafe fn coverage_observer<'a>(
    name: &'static str,
    ptr: *mut u8,
    len: usize,
) -> libafl::observers::HitcountsMapObserver<libafl::observers::StdMapObserver<'a, u8, false>> {
    libafl::observers::HitcountsMapObserver::new(libafl::observers::StdMapObserver::from_mut_ptr(
        name, ptr, len,
    ))
}

#[cfg(test)]
mod coverage_lifecycle_tests {
    use super::coverage_observer;
    use libafl::{executors::ExitKind, observers::Observer};
    #[test]
    fn target_maps_reset_without_feedback_and_preserve_other_target() {
        let mut a = [0_u8; 4];
        let mut b = [0_u8; 4];
        let mut oa = unsafe { coverage_observer("a", a.as_mut_ptr(), 4) };
        let mut ob = unsafe { coverage_observer("b", b.as_mut_ptr(), 4) };
        for previous_exit in [ExitKind::Ok, ExitKind::Crash, ExitKind::Timeout] {
            oa.pre_exec(&mut (), &()).unwrap();
            a[0] = 3;
            oa.post_exec(&mut (), &(), &previous_exit).unwrap();
            ob.pre_exec(&mut (), &()).unwrap();
            b[1] = 1;
            ob.post_exec(&mut (), &(), &previous_exit).unwrap();
            assert_eq!(a, [4, 0, 0, 0]);
            // No feedback append: covers rejected inputs, solutions and calibration.
            for _ in 0..2 {
                oa.pre_exec(&mut (), &()).unwrap();
                a[2] = 3;
                oa.post_exec(&mut (), &(), &ExitKind::Ok).unwrap();
                ob.pre_exec(&mut (), &()).unwrap();
                b[3] = 1;
                ob.post_exec(&mut (), &(), &ExitKind::Ok).unwrap();
                assert_eq!(a, [0, 0, 4, 0]);
                assert_eq!(b, [0, 0, 0, 1]);
            }
        }
    }
}
