use std::env;
use std::io::Write;

use libafl_cc::{ClangWrapper, CompilerWrapper, ToolWrapper};
use tempfile::NamedTempFile;

#[path = "../profile.rs"]
mod profile;
use profile::TargetProfile;

pub fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
        let target_profile = TargetProfile::from_env().unwrap_or_else(|message| {
            eprintln!("{message}");
            std::process::exit(2);
        });
        let mut dir = env::current_exe().unwrap();
        let wrapper_name = dir.file_name().unwrap().to_str().unwrap();

        let is_cpp = match wrapper_name[wrapper_name.len()-2..].to_lowercase().as_str() {
            "cc" => false,
            "++" | "pp" | "xx" => true,
            _ => panic!("Could not figure out if c or c++ wrapper was called. Expected {dir:?} to end with c or cxx"),
        };

        dir.pop();

        let mut cc = ClangWrapper::new();
        // Create a macro definition for __AFL_FUZZ_INIT that is C++-aware.
        let fuzz_init_def = if is_cpp {
            // For C++ targets, declare libafl_start_forkserver with extern "C"
            // so it matches the Rust-exported C linkage symbol.
            "-D__AFL_FUZZ_INIT()=int __afl_sharedmem_fuzzing = 1;extern unsigned int *__afl_fuzz_len;extern unsigned char *__afl_fuzz_ptr;unsigned char __afl_fuzz_alt[1048576];unsigned char *__afl_fuzz_alt_ptr = __afl_fuzz_alt;extern \"C\" void libafl_start_forkserver(void)"
        } else {
            "-D__AFL_FUZZ_INIT()=int __afl_sharedmem_fuzzing = 1;extern unsigned int *__afl_fuzz_len;extern unsigned char *__afl_fuzz_ptr;unsigned char __afl_fuzz_alt[1048576];unsigned char *__afl_fuzz_alt_ptr = __afl_fuzz_alt;void libafl_start_forkserver(void)"
        };

        // Generate a small temporary header with rv_shm declarations and include it.
        // This mirrors how we add `libafl_start_forkserver` to the wrapper: create
        // a tiny header that declares the C symbols implemented in the compiler
        // crate (rv_shm.rs) and force-include it so C/C++ targets can call them.
        // Using a temp header avoids fragile -D quoting on the command line.
        // Create a NamedTempFile so it's automatically removed when dropped.
        let mut tmpfile: NamedTempFile = NamedTempFile::new().expect("failed to create temp file");
        let tmp_header_path = tmpfile.path().to_path_buf();
        let header_contents = if is_cpp {
            "#ifdef __cplusplus\nextern \"C\" {\n#endif\n#include <stddef.h>\n#include <stdint.h>\nint pairfuzz_result_init(void);\nvoid pairfuzz_publish_result(uint64_t value);\n#ifdef __cplusplus\n}\n#endif\n"
        } else {
            "#include <stddef.h>\n#include <stdint.h>\nint pairfuzz_result_init(void);\nvoid pairfuzz_publish_result(uint64_t value);\n"
        };
        // Write header contents into the NamedTempFile (best-effort)
        let _ = tmpfile.write_all(header_contents.as_bytes());

        cc.cpp(is_cpp)
            // silence the compiler wrapper output, needed for some configure scripts.
            .silence(true)
            .parse_args(&args)
            .expect("Failed to parse the command line");

        if target_profile.uses_asan() {
            cc.add_arg("-fsanitize=address");
        }

        if let Some(code) = cc
            // Coverage remains enabled in both sanitizer profiles.
            .add_arg("-fsanitize-coverage=trace-pc-guard")
            // Imitate afl-cc's compile definitions
            .add_arg(fuzz_init_def)
            .add_arg("-D__AFL_FUZZ_TESTCASE_BUF=(__afl_fuzz_ptr ? __afl_fuzz_ptr : __afl_fuzz_alt_ptr)")
            .add_arg("-D__AFL_FUZZ_TESTCASE_LEN=(__afl_fuzz_ptr ? *__afl_fuzz_len : (*__afl_fuzz_len = read(0, __afl_fuzz_alt_ptr, 1048576)) == 0xffffffff ? 0 : *__afl_fuzz_len)")
            .add_arg("-D__AFL_INIT()=libafl_start_forkserver()")
            // Ensure necessary headers are force-included so PAIRFUZZ macros can use them
            .add_arg("-include")
            .add_arg("unistd.h")
            // Force-include the runtime-generated header with rv_shm declarations
            .add_arg("-include")
            .add_arg(tmp_header_path.to_str().unwrap())
            // PAIRFUZZ helper macros keep legacy source compatibility while the
            // runtime publishes an eight-byte semantic value through a cached map.
            .add_arg("-DPAIRFUZZ_INIT()=__AFL_FUZZ_INIT()")
            .add_arg("-DPAIRFUZZ_RETVAL(r)=do{int32_t __d_legacy=(int32_t)(r);pairfuzz_publish_result((uint64_t)(uint32_t)__d_legacy);}while(0)")
            // Provide convenience macros PAIRFUZZ_RUN(fn) and PAIRFUZZ_CALL(fn,out)
            // PAIRFUZZ_RUN(fn) executes fn(buf,len), writes the result, and returns it.
            .add_arg("-DPAIRFUZZ_RUN(fn)=do{if(pairfuzz_result_init()!=0)return 90;__AFL_INIT();uint8_t*__d_buf=(uint8_t*)__AFL_FUZZ_TESTCASE_BUF;size_t __d_len=__AFL_FUZZ_TESTCASE_LEN;int __d_result=(fn)(__d_buf,__d_len);PAIRFUZZ_RETVAL(__d_result);return __d_result;}while(0)")
            .add_arg("-DPAIRFUZZ_RUN_SEMANTIC(fn)=do{if(pairfuzz_result_init()!=0)return 90;__AFL_INIT();uint8_t*__d_buf=(uint8_t*)__AFL_FUZZ_TESTCASE_BUF;size_t __d_len=__AFL_FUZZ_TESTCASE_LEN;uint64_t __d_result=(uint64_t)(fn)(__d_buf,__d_len);pairfuzz_publish_result(__d_result);return 0;}while(0)")
            .add_arg("-DPAIRFUZZ_CALL(fn,out)=do{if(pairfuzz_result_init()!=0)return 90;uint8_t*__d_buf=(uint8_t*)__AFL_FUZZ_TESTCASE_BUF;size_t __d_len=__AFL_FUZZ_TESTCASE_LEN;(out)=(fn)(__d_buf,__d_len);PAIRFUZZ_RETVAL(out);}while(0)")
            // Link with the compiler crate staticlib which exports both the
            // forkserver start symbol and the rv_shm helper symbols (rv_shm.rs).
            // This makes the PairFuzz semantic-result runtime available to
            // instrumented C/C++ targets.
            .link_staticlib(&dir, "libforkserver_libafl_cc")
            .run()
            .expect("Failed to run the wrapped compiler")
        {
            std::process::exit(code);
        }
        // tmpfile is dropped here when it goes out of scope, removing the temp file
    } else {
        panic!("LibAFL CC: No Arguments given");
    }
}
