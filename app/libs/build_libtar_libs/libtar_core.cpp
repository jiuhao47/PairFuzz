#include "libtar.h"
#include <dirent.h>
#include <errno.h>
#include <fcntl.h>
#include <fstream>
#include <iostream>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <sys/mman.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>
#include <vector>

#ifndef MFD_CLOEXEC
#define MFD_CLOEXEC 0x0001U
#endif

// 递归删除目录及其内容（更健壮：使用 stat 检查类型，并在失败时尝试调整权限）
void remove_directory(const char *path) {
  DIR *dir = opendir(path);
  if (dir == nullptr) {
    // 如果无法打开目录，尝试直接 unlink/rmdir
    if (unlink(path) != 0) {
      // not a file or cannot unlink
    }
    return;
  }

  struct dirent *entry;
  while ((entry = readdir(dir)) != nullptr) {
    if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
      continue;
    }

    std::string full_path = std::string(path) + "/" + entry->d_name;
    struct stat st;
    if (stat(full_path.c_str(), &st) != 0) {
      // can't stat, try unlink and continue
      unlink(full_path.c_str());
      continue;
    }

    if (S_ISDIR(st.st_mode)) {
      // 目录，递归删除
      remove_directory(full_path.c_str());
      // 尝试删除目录本身
      if (rmdir(full_path.c_str()) != 0) {
        // 可能是权限问题，尝试修复权限后再删
        chmod(full_path.c_str(), S_IRWXU);
        rmdir(full_path.c_str());
      }
    } else {
      // 文件或其他，尝试 unlink
      if (unlink(full_path.c_str()) != 0) {
        // 尝试调整权限后再删
        chmod(full_path.c_str(), S_IRWXU);
        unlink(full_path.c_str());
      }
    }
  }

  closedir(dir);

  // 删除空目录
  if (rmdir(path) != 0) {
    // 尝试调整权限再删
    chmod(path, S_IRWXU);
    rmdir(path);
  }
}

// 全局注册的临时目录列表，用于 atexit 清理
static std::vector<std::string> g_temp_dirs;

static void cleanup_all_temp_dirs() {
  for (const auto &d : g_temp_dirs) {
    remove_directory(d.c_str());
    rmdir(d.c_str());
  }
  g_temp_dirs.clear();
}

// Scan and remove old temporary directories in a parent tmp (e.g. /tmp).
// Matches names starting with prefixes in `prefixes` and removes entries
// whose mtime is older than `age_seconds`. Default age is 300s but can be
// overridden with DIFFRAY_CLEAN_AGE env var.
static void cleanup_old_tmp_dirs(const char *tmp_parent,
                                 int default_age_seconds = 300) {
  const char *env = getenv("DIFFRAY_CLEAN_AGE");
  int age = default_age_seconds;
  if (env) {
    int v = atoi(env);
    if (v > 0)
      age = v;
  }

  const char *prefixes[] = {"diffray-", "magick-", NULL};
  DIR *d = opendir(tmp_parent);
  if (!d)
    return;
  struct dirent *ent;
  time_t now = time(NULL);
  while ((ent = readdir(d)) != NULL) {
    if (strcmp(ent->d_name, ".") == 0 || strcmp(ent->d_name, "..") == 0)
      continue;
    bool match = false;
    for (const char **p = prefixes; *p; ++p) {
      if (strncmp(ent->d_name, *p, strlen(*p)) == 0) {
        match = true;
        break;
      }
    }
    if (!match)
      continue;

    // Build full path and stat
    char full[PATH_MAX];
    snprintf(full, sizeof(full), "%s/%s", tmp_parent, ent->d_name);
    struct stat st;
    if (stat(full, &st) != 0)
      continue;
    if (!S_ISDIR(st.st_mode))
      continue;
    if ((now - st.st_mtime) < age)
      continue;

    // attempt to remove recursively; ignore errors
    remove_directory(full);
    rmdir(full);
  }
  closedir(d);
}

// RAII helper: create a temporary directory and remove it on destruction.
// Chooses DIFFRAY_TMP -> XDG_RUNTIME_DIR -> ./ as base.
class ScopedTempDir {
public:
  ScopedTempDir() : tmpdir_path_() {
    // cleanup old leftover tmp dirs on first ScopedTempDir creation
    cleanup_old_tmp_dirs("/tmp", 300);
    const char *base = getenv("DIFFRAY_TMP");
    if (!base)
      base = getenv("XDG_RUNTIME_DIR");
    std::string base_dir =
        (base && base[0] != '\0') ? std::string(base) : std::string(".");
    // reduce collision risk: include pid and nanosecond timestamp in template
    struct timespec _ts;
    clock_gettime(CLOCK_REALTIME, &_ts);
    pid_t _pid = getpid();
    std::string tmpl = base_dir + "/diffray-" + std::to_string(_pid) + "-" +
                       std::to_string(_ts.tv_nsec) + "-XXXXXX";
    // mkdtemp needs a mutable buffer
    std::vector<char> buf(tmpl.begin(), tmpl.end());
    buf.push_back('\0');
    if (mkdtemp(buf.data()) != nullptr) {
      tmpdir_path_ = std::string(buf.data());
      // set TMPDIR so libraries that respect it will use our dir
      setenv("TMPDIR", tmpdir_path_.c_str(), 1);
      // tighten permissions to owner only
      chmod(tmpdir_path_.c_str(), S_IRWXU);
      // record for atexit cleanup
      g_temp_dirs.push_back(tmpdir_path_);
      static bool registered = false;
      if (!registered) {
        atexit(cleanup_all_temp_dirs);
        registered = true;
      }
    }
  }

  ~ScopedTempDir() {
    if (!tmpdir_path_.empty()) {
      remove_directory(tmpdir_path_.c_str());
      rmdir(tmpdir_path_.c_str());
      // unset TMPDIR to avoid affecting the rest of the process
      unsetenv("TMPDIR");
    }
  }

  const char *path() const {
    return tmpdir_path_.empty() ? nullptr : tmpdir_path_.c_str();
  }

  // create a temp file inside this directory; returns fd and sets out_path
  int create_temp_file(std::string &out_path,
                       const char *prefix = "fuzz_tar_") {
    if (tmpdir_path_.empty())
      return -1;
    // include pid and timestamp to reduce mkstemp collisions under concurrency
    struct timespec _ts2;
    clock_gettime(CLOCK_REALTIME, &_ts2);
    pid_t _pid2 = getpid();
    std::string p = std::string(tmpdir_path_) + "/" + prefix +
                    std::to_string(_pid2) + "-" + std::to_string(_ts2.tv_nsec) +
                    "-XXXXXX";
    std::vector<char> buf(p.begin(), p.end());
    buf.push_back('\0');
    int fd = mkstemp(buf.data());
    if (fd >= 0) {
      out_path = std::string(buf.data());
    }
    return fd;
  }

private:
  std::string tmpdir_path_;
};

extern "C" int tar_parse(const uint8_t *data, size_t size) {
  TAR *ptar_handle = NULL;
  int iret = -1;
  int ret = 1;

  // 创建一个临时文件，用于存储模糊测试输入
  // 优先使用环境变量 DIFFRAY_TMP，其次
  // XDG_RUNTIME_DIR；如果都没设置，降级到当前目录，避免固定写入 /tmp
  // 优先尝试使用
  // memfd（Linux）避免在磁盘上创建临时文件；若不可用则回退到文件方案
  ScopedTempDir tdir;
  int memfd = -1;
  char procfd_path[64] = {0};
  const char *tar_filename = nullptr;
#ifdef __linux__
// 使用 syscall 以兼容不同 libc 版本
#ifdef SYS_memfd_create
  memfd = syscall(SYS_memfd_create, "fuzz_tar", MFD_CLOEXEC);
#endif
  if (memfd >= 0) {
    if (write(memfd, data, size) != (ssize_t)size) {
      perror("write");
      close(memfd);
      return 1;
    }
    if (lseek(memfd, 0, SEEK_SET) == -1) {
      perror("lseek");
      close(memfd);
      return 1;
    }
    snprintf(procfd_path, sizeof(procfd_path), "/proc/self/fd/%d", memfd);
    tar_filename = procfd_path;
  }
#endif

  std::string tmp_filename;
  int fd = -1;
  if (tar_filename == nullptr) {
    // memfd 不可用或创建失败，回退到在临时目录下创建文件
    fd = tdir.create_temp_file(tmp_filename);
    if (fd < 0) {
      struct timespec _ts3;
      clock_gettime(CLOCK_REALTIME, &_ts3);
      pid_t _pid3 = getpid();
      std::string fallback = std::string("./fuzz_tar-") +
                             std::to_string(_pid3) + "-" +
                             std::to_string(_ts3.tv_nsec) + "-XXXXXX";
      std::vector<char> buf(fallback.begin(), fallback.end());
      buf.push_back('\0');
      fd = mkstemp(buf.data());
      if (fd < 0) {
        perror("mkstemp");
        return 1;
      }
      tmp_filename = std::string(buf.data());
    }

    // 将模糊测试输入写入临时文件
    if (write(fd, data, size) != (ssize_t)size) {
      perror("write");
      close(fd);
      unlink(tmp_filename.c_str());
      return 1;
    }
    close(fd);
    tar_filename = tmp_filename.c_str();
  }
  // 在 ScopedTempDir 下创建一个子目录用于解压
  std::string tmp_dir_template;
  if (tdir.path()) {
    struct timespec _ts4;
    clock_gettime(CLOCK_REALTIME, &_ts4);
    pid_t _pid4 = getpid();
    tmp_dir_template = std::string(tdir.path()) + "/fuzz_tar_extract-" +
                       std::to_string(_pid4) + "-" +
                       std::to_string(_ts4.tv_nsec) + "-XXXXXX";
  } else {
    struct timespec _ts4;
    clock_gettime(CLOCK_REALTIME, &_ts4);
    pid_t _pid4 = getpid();
    tmp_dir_template = std::string("./fuzz_tar_extract-") +
                       std::to_string(_pid4) + "-" +
                       std::to_string(_ts4.tv_nsec) + "-XXXXXX";
  }
  std::vector<char> tmp_dir_buf(tmp_dir_template.begin(),
                                tmp_dir_template.end());
  tmp_dir_buf.push_back('\0');
  if (mkdtemp(tmp_dir_buf.data()) == nullptr) {
    perror("mkdtemp");
    // 清理 memfd 或临时文件
    if (memfd >= 0)
      close(memfd);
    else
      unlink(tmp_filename.c_str());
    return 1;
  }
  std::string tmp_dir = std::string(tmp_dir_buf.data());

  // 打开 tar 文件
  iret = tar_open(&ptar_handle, tar_filename, NULL, O_RDONLY, 0644, TAR_GNU);
  // TAR_IGNORE_CRC
  if (iret != 0) {
    // 打开失败，清理临时文件和目录
    if (memfd >= 0)
      close(memfd);
    else
      unlink(tmp_filename.c_str());
    remove_directory(tmp_dir.c_str());
    return 1;
  }

  // 尝试解压所有文件
  // libtar expects a `char *` for the prefix; pass the mutable buffer used by
  // mkdtemp
  if (tar_extract_all(ptar_handle, tmp_dir_buf.data()) == 0) {
    // 如果解压成功，打印 success
    // printf("success\n");
    ret = 0;
  }

  // 关闭 tar 句柄并清理资源
  tar_close(ptar_handle);
  if (memfd >= 0)
    close(memfd);
  else
    unlink(tmp_filename.c_str());
  remove_directory(tmp_dir.c_str());

  return ret;
}