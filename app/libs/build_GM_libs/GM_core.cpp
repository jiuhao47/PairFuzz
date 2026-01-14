#include <Magick++.h>
#include <cstdint>
#include <dirent.h>
#include <fstream>
#include <iostream>
#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <time.h>
#include <unistd.h>
#include <vector>

#include "magick/describe.h"
// Core implementation extracted from GM_harness.cpp
using namespace std;

// Helper: recursively remove a directory tree (limited depth)
static void remove_directory_recursive(const char *path, int depth = 0) {
  if (depth > 10)
    return; // guard against too deep recursion
  DIR *dir = opendir(path);
  if (!dir)
    return;
  struct dirent *entry;
  char child[PATH_MAX];
  while ((entry = readdir(dir)) != NULL) {
    if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0)
      continue;
    snprintf(child, sizeof(child), "%s/%s", path, entry->d_name);
    struct stat st;
    if (stat(child, &st) != 0)
      continue;
    if (S_ISDIR(st.st_mode)) {
      remove_directory_recursive(child, depth + 1);
      rmdir(child);
    } else {
      unlink(child);
    }
  }
  closedir(dir);
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

  const char *prefixes[] = {"diffray-gm-", "magick-", NULL};
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
    remove_directory_recursive(full, 0);
    rmdir(full);
  }
  closedir(d);
}

// RAII helper: create temporary directory and set TMPDIR/MAGICK_TMPDIR for
// the current process. Restores previous envs and deletes the directory on
// destruction.
class ScopedTempDir {
public:
  ScopedTempDir() : tmpdir_path_{}, old_tmp_{}, old_magick_tmp_{} {
    // cleanup old leftover tmp dirs on first ScopedTempDir creation
    cleanup_old_tmp_dirs("/tmp", 300);
    // build a less-colliding template using pid and nanosecond timestamp
    struct timespec _ts;
    clock_gettime(CLOCK_REALTIME, &_ts);
    pid_t _pid = getpid();
    std::string tmpl = std::string("/tmp/diffray-gm-") + std::to_string(_pid) +
                       "-" + std::to_string(_ts.tv_nsec) + "-XXXXXX";
    std::vector<char> tmpl_buf(tmpl.begin(), tmpl.end());
    tmpl_buf.push_back('\0');
    char *p = mkdtemp(tmpl_buf.data());
    if (p) {
      tmpdir_path_ = string(p);
      // save old env values
      char *t = getenv("TMPDIR");
      if (t)
        old_tmp_ = string(t);
      char *m = getenv("MAGICK_TMPDIR");
      if (m)
        old_magick_tmp_ = string(m);
      setenv("TMPDIR", tmpdir_path_.c_str(), 1);
      setenv("MAGICK_TMPDIR", tmpdir_path_.c_str(), 1);
    }
  }

  ~ScopedTempDir() {
    // restore env
    if (!old_tmp_.empty())
      setenv("TMPDIR", old_tmp_.c_str(), 1);
    else
      unsetenv("TMPDIR");
    if (!old_magick_tmp_.empty())
      setenv("MAGICK_TMPDIR", old_magick_tmp_.c_str(), 1);
    else
      unsetenv("MAGICK_TMPDIR");

    // remove directory contents and directory itself
    if (!tmpdir_path_.empty()) {
      remove_directory_recursive(tmpdir_path_.c_str());
      rmdir(tmpdir_path_.c_str());
    }
  }

  const char *path() const {
    return tmpdir_path_.empty() ? nullptr : tmpdir_path_.c_str();
  }

private:
  string tmpdir_path_;
  string old_tmp_;
  string old_magick_tmp_;
};

class MagickStateOLD {
public:
  MagickStateOLD() {
    // printf("init at startup for oldgm\n");
    Magick::InitializeMagick(nullptr);
    MagickLib::SetMagickResourceLimit(MagickLib::MemoryResource, 1000000000);
    MagickLib::SetMagickResourceLimit(MagickLib::WidthResource, 2048);
    MagickLib::SetMagickResourceLimit(MagickLib::HeightResource, 2048);
  }
};

// Static initializer so this code is run once at startup.
MagickStateOLD kMagickState2;

int basic_encode_error(const char *str) {
  const std::string target(str);
  size_t idx = target.find("reported by");
  // no such report
  if (idx == std::string::npos) {
    return -1;
  }
  size_t funcIdx = target.rfind("(");
  size_t value = 0;
  for (int i = idx + 12; i < funcIdx - 1; i++) {
    value += (i - (idx + 12)) * str[i];
  }
  // return (value==0) ? 0 : 1;
  return value;
}

extern "C" int GM_parse(const uint8_t *data, size_t size) {
  Magick::Image image;
  const Magick::Blob blob(data, size);

  // Use a process-private temporary directory so Magick doesn't write into
  // the global /tmp. ScopedTempDir will set TMPDIR/MAGICK_TMPDIR and clean up.
  ScopedTempDir tdir;

  // Try to use memfd_create on Linux to avoid creating on-disk temp files.
  int memfd = -1;
  char procfd_path[64] = {0};
#ifdef __linux__
#ifdef SYS_memfd_create
  memfd = syscall(SYS_memfd_create, "gm_fuzz", 0);
#endif
  if (memfd >= 0) {
    if (write(memfd, data, size) != (ssize_t)size) {
      perror("write(memfd)");
      close(memfd);
      memfd = -1;
    } else {
      if (lseek(memfd, 0, SEEK_SET) == -1) {
        perror("lseek(memfd)");
        close(memfd);
        memfd = -1;
      } else {
        snprintf(procfd_path, sizeof(procfd_path), "/proc/self/fd/%d", memfd);
      }
    }
  }
#endif

  try {
    if (memfd >= 0 && procfd_path[0] != '\0') {
      // Read from the memfd via /proc/self/fd/<fd>
      image.read(std::string(procfd_path));
    } else {
      // Fallback to Blob-based read (original behavior)
      image.read(blob);
    }
  } catch (Magick::Exception &error_) {
    if (memfd >= 0)
      close(memfd);
    return basic_encode_error(error_.what());
  }

  if (memfd >= 0)
    close(memfd);
  return 0;
}
