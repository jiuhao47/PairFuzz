use std::env;

use libafl_cc::{ClangWrapper, CompilerWrapper, ToolWrapper};

pub fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() > 1 {
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
        let tmp_dir = std::env::temp_dir();
        let tmp_header_path =
            tmp_dir.join(format!("diff_ray_rv_shm_decl_{}.h", std::process::id()));
        let header_contents = if is_cpp {
            "#ifdef __cplusplus\nextern \"C\" {\n#endif\n#include <stddef.h>\n#include <stdint.h>\nvoid select_rv_shm_env(const char **out_id, const char **out_size);\nvoid write_return_value_to_shm(const char *env_id, const char *env_size, int32_t result);\n#ifdef __cplusplus\n}\n#endif\n"
        } else {
            "#include <stddef.h>\n#include <stdint.h>\nvoid select_rv_shm_env(const char **out_id, const char **out_size);\nvoid write_return_value_to_shm(const char *env_id, const char *env_size, int32_t result);\n"
        };
        // Write header file (best-effort, ignore errors here and let compiler fail later if necessary)
        let _ = std::fs::write(&tmp_header_path, header_contents);

        if let Some(code) = cc
            .cpp(is_cpp)
            // silence the compiler wrapper output, needed for some configure scripts.
            .silence(true)
            .parse_args(&args)
            .expect("Failed to parse the command line")
            // Enable libafl's coverage instrumentation
            .add_arg("-fsanitize=address")
            .add_arg("-fsanitize-coverage=trace-pc-guard")
            // Imitate afl-cc's compile definitions
            .add_arg(fuzz_init_def)
            .add_arg("-D__AFL_FUZZ_TESTCASE_BUF=(__afl_fuzz_ptr ? __afl_fuzz_ptr : __afl_fuzz_alt_ptr)")
            .add_arg("-D__AFL_FUZZ_TESTCASE_LEN=(__afl_fuzz_ptr ? *__afl_fuzz_len : (*__afl_fuzz_len = read(0, __afl_fuzz_alt_ptr, 1048576)) == 0xffffffff ? 0 : *__afl_fuzz_len)")
            .add_arg("-D__AFL_INIT()=libafl_start_forkserver()")
            // Ensure necessary headers are force-included so DIFFRAY macros can use them
            .add_arg("-include")
            .add_arg("unistd.h")
            // Force-include the runtime-generated header with rv_shm declarations
            .add_arg("-include")
            .add_arg(tmp_header_path.to_str().unwrap())
            // DIFFRAY helper macros: DIFFRAY_INIT() expands to the AFL init macro;
            // DIFFRAY_RETVAL(r) will select the rv shm env and write the provided r.
            .add_arg("-DDIFFRAY_INIT()=__AFL_FUZZ_INIT()")
            .add_arg("-DDIFFRAY_RETVAL(r)=do{const char*__d_id=NULL;const char*__d_size=NULL;select_rv_shm_env(&__d_id,&__d_size);write_return_value_to_shm(__d_id,__d_size,(r));}while(0)")
            // Provide convenience macros DIFFRAY_RUN(fn) and DIFFRAY_CALL(fn,out)
            // DIFFRAY_RUN(fn) executes fn(buf,len), writes the result, and returns it.
            .add_arg("-DDIFFRAY_RUN(fn)=do{__AFL_INIT();uint8_t*__d_buf=(uint8_t*)__AFL_FUZZ_TESTCASE_BUF;size_t __d_len=__AFL_FUZZ_TESTCASE_LEN;int __d_result=(fn)(__d_buf,__d_len);DIFFRAY_RETVAL(__d_result);return __d_result;}while(0)")
            .add_arg("-DDIFFRAY_CALL(fn,out)=do{uint8_t*__d_buf=(uint8_t*)__AFL_FUZZ_TESTCASE_BUF;size_t __d_len=__AFL_FUZZ_TESTCASE_LEN;(out)=(fn)(__d_buf,__d_len);DIFFRAY_RETVAL(out);}while(0)")
            // Link with the compiler crate staticlib which exports both the
            // forkserver start symbol and the rv_shm helper symbols (rv_shm.rs).
            // This makes select_rv_shm_env/write_return_value_to_shm available
            // to instrumented C/C++ targets.
            .link_staticlib(&dir, "libforkserver_libafl_cc")
            .run()
            .expect("Failed to run the wrapped compiler")
        {
            std::process::exit(code);
        }
    } else {
        panic!("LibAFL CC: No Arguments given");
    }
}
