use libafl::{
    executors::{forkserver::ForkserverExecutor, StdChildArgs},
    inputs::Input,
    mutators::Tokens,
    observers::{HitcountsMapObserver, StdMapObserver},
    state::HasExecutions,
};
use libafl_bolts::{
    shmem::{StdShMem, StdShMemProvider},
    tuples::tuple_list,
    StdTargetArgs,
};
use std::{path::Path, time::Duration};

use crate::{
    observers::ReturnValueObserver,
    utils::logging::{error, info, warn},
};

pub(crate) type CoverageObserver = HitcountsMapObserver<StdMapObserver<'static, u8, true>>;

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
            rv_env: "__DIFFRAY_RV_SHM_ID_A",
            rv_size_env: "__DIFFRAY_RV_SHM_ID_A_SIZE",
            rv_temp_env: "__DIFFRAY_TMP_RV_A",
            coverage_temp_env: "__DIFFRAY_TMP_COV_A",
        },
        TargetRole::B => TargetNames {
            label: "B",
            return_observer: "second_return",
            rv_env: "__DIFFRAY_RV_SHM_ID_B",
            rv_size_env: "__DIFFRAY_RV_SHM_ID_B_SIZE",
            rv_temp_env: "__DIFFRAY_TMP_RV_B",
            coverage_temp_env: "__DIFFRAY_TMP_COV_B",
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

pub(crate) fn build_executor<I, S>(
    spec: TargetSpec<'_>,
    map_observer: CoverageObserver,
    mut rv_observer: ReturnValueObserver,
    provider: &mut StdShMemProvider,
    tokens: &mut Tokens,
) -> ForkserverExecutor<I, (CoverageObserver, (ReturnValueObserver, ())), S, StdShMem>
where
    I: Input,
    S: HasExecutions,
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
        rv_observer.set_shmem(return_shmem);
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
        assert_eq!(value.rv_env, "__DIFFRAY_RV_SHM_ID_A");
        assert_eq!(value.rv_size_env, "__DIFFRAY_RV_SHM_ID_A_SIZE");
        assert_eq!(value.rv_temp_env, "__DIFFRAY_TMP_RV_A");
        assert_eq!(value.coverage_temp_env, "__DIFFRAY_TMP_COV_A");
    }

    #[test]
    fn target_role_b_uses_only_b_names() {
        let value = names(TargetRole::B);
        assert_eq!(value.label, "B");
        assert_eq!(value.return_observer, "second_return");
        assert_eq!(value.rv_env, "__DIFFRAY_RV_SHM_ID_B");
        assert_eq!(value.rv_size_env, "__DIFFRAY_RV_SHM_ID_B_SIZE");
        assert_eq!(value.rv_temp_env, "__DIFFRAY_TMP_RV_B");
        assert_eq!(value.coverage_temp_env, "__DIFFRAY_TMP_COV_B");
    }
}
