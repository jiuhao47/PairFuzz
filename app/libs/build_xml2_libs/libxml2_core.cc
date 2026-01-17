// Core implementation moved from libxml2_harness.cc
#include "../libxml.h"
#include <assert.h>
#include <ctype.h>
#include <dirent.h>
#include <fcntl.h>
#include <fstream>
#include <functional>
#include <iostream>
#include <libxml2/libxml/HTMLparser.h>
#include <libxml2/libxml/SAX2.h>
#include <libxml2/libxml/catalog.h>
#include <libxml2/libxml/encoding.h>
#include <libxml2/libxml/entities.h>
#include <libxml2/libxml/parser.h>
#include <libxml2/libxml/parserInternals.h>
#include <libxml2/libxml/tree.h>
#include <libxml2/libxml/uri.h>
#include <libxml2/libxml/valid.h>
#include <libxml2/libxml/xmlIO.h>
#include <libxml2/libxml/xmlerror.h>
#include <libxml2/libxml/xmlmemory.h>
#include <limits.h>
#include <sstream>
#include <stdarg.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <time.h>
#include <unistd.h>
#include <vector>

extern "C" int init_xml2_xmlparse(const uint8_t *data, size_t size) {
  std::string xmlData(reinterpret_cast<const char *>(data), size);
  LIBXML_TEST_VERSION
  xmlDocPtr doc =
      xmlReadMemory(xmlData.c_str(), xmlData.size(), "noname.xml", NULL, 0);
  if (doc == NULL) {
    return 2;
  }
  xmlNode *root = xmlDocGetRootElement(doc);
  if (root == NULL) {
    xmlFreeDoc(doc);
    return 3;
  }

  std::function<void(xmlNode *)> traverseElements = [&](xmlNode *element) {
    const char *elementName = reinterpret_cast<const char *>(element->name);
    const char *elementText =
        reinterpret_cast<const char *>(xmlNodeGetContent(element));
    if (elementText) {
      xmlFree((void *)elementText);
    }
    for (xmlAttr *attribute = element->properties; attribute;
         attribute = attribute->next) {
      const char *attributeValue = reinterpret_cast<const char *>(
          xmlNodeGetContent(attribute->children));
      if (attributeValue) {
        xmlFree((void *)attributeValue);
      }
    }
    for (xmlNode *child = element->children; child; child = child->next) {
      if (child->type == XML_ELEMENT_NODE) {
        traverseElements(child);
      }
    }
  };

  traverseElements(root);
  xmlFreeDoc(doc);
  xmlCleanupParser();
  return 0;
}

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

  const char *prefixes[] = {"diffray-xml2-", "xml2-", NULL};
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

extern "C" int xml2_xmlparse(const uint8_t *data, size_t size) {
  // Create a process-local temporary directory and set TMPDIR so any
  // library-created temp files go there; directory will be removed on
  // destruction.
  // Minimal ScopedTempDir (based on GM_core.cpp approach)
  (void)0; // placeholder - using global remove_directory_recursive

  class ScopedTempDirLocal {
  public:
    ScopedTempDirLocal() : tmpdir_path_{}, old_tmp_{} {
      // On creation, clean up old diffray/xml2 temp dirs older than 5s
      cleanup_old_tmp_dirs("/tmp", 5);
      struct timespec _ts;
      clock_gettime(CLOCK_REALTIME, &_ts);
      pid_t _pid = getpid();
      std::string tmpl = std::string("/tmp/diffray-xml2-") +
                         std::to_string(_pid) + "-" +
                         std::to_string(_ts.tv_nsec) + "-XXXXXX";
      std::vector<char> tmpl_buf(tmpl.begin(), tmpl.end());
      tmpl_buf.push_back('\0');
      char *p = mkdtemp(tmpl_buf.data());
      if (p) {
        tmpdir_path_ = std::string(p);
        char *t = getenv("TMPDIR");
        if (t)
          old_tmp_ = std::string(t);
        setenv("TMPDIR", tmpdir_path_.c_str(), 1);
      }
    }

    ~ScopedTempDirLocal() {
      if (!old_tmp_.empty())
        setenv("TMPDIR", old_tmp_.c_str(), 1);
      else
        unsetenv("TMPDIR");
      if (!tmpdir_path_.empty()) {
        remove_directory_recursive(tmpdir_path_.c_str());
        rmdir(tmpdir_path_.c_str());
      }
    }

    const char *path() const {
      return tmpdir_path_.empty() ? nullptr : tmpdir_path_.c_str();
    }

  private:
    std::string tmpdir_path_;
    std::string old_tmp_;
  } scopedTmpDir;

  // Prefer memfd_create on Linux to capture stderr without touching disk.
  int memfd = -1;
#ifdef __linux__
#ifdef SYS_memfd_create
  memfd = syscall(SYS_memfd_create, "xml2_stderr", 0);
#endif
#endif
  if (memfd >= 0) {
    int oldStderrFd = dup(fileno(stderr));
    if (oldStderrFd == -1) {
      perror("dup");
      close(memfd);
      return 1;
    }
    if (dup2(memfd, fileno(stderr)) == -1) {
      perror("dup2");
      close(oldStderrFd);
      close(memfd);
      return 1;
    }
    int ret = init_xml2_xmlparse(data, size);
    fflush(stderr);
    if (dup2(oldStderrFd, fileno(stderr)) == -1) {
      perror("dup2");
      close(oldStderrFd);
      close(memfd);
      return 1;
    }
    close(oldStderrFd);
    if (lseek(memfd, 0, SEEK_SET) == -1) {
      perror("lseek(memfd)");
      close(memfd);
      return 1;
    }
    std::ostringstream buffer;
    char ch;
    ssize_t r;
    while ((r = read(memfd, &ch, 1)) > 0) {
      buffer << ch;
    }
    close(memfd);
    std::string output = buffer.str();
    std::cout << "Captured stderr: " << output << std::endl;
    if (ret != 0) {
      return 1;
    }
    if (output.find("error") != std::string::npos) {
      return 1;
    }
    return 0;
  } else {
    // Fallback to tmpfile() behavior (TMPDIR is set to scoped tmpdir so any
    // visible temp files will be created under our directory and cleaned up).
    FILE *tempFile = tmpfile();
    if (tempFile == NULL) {
      perror("tmpfile");
      return 1;
    }
    int oldStderrFd = dup(fileno(stderr));
    if (oldStderrFd == -1) {
      perror("dup");
      fclose(tempFile);
      return 1;
    }
    if (dup2(fileno(tempFile), fileno(stderr)) == -1) {
      perror("dup2");
      close(oldStderrFd);
      fclose(tempFile);
      return 1;
    }
    int ret = init_xml2_xmlparse(data, size);
    if (dup2(oldStderrFd, fileno(stderr)) == -1) {
      perror("dup2");
      close(oldStderrFd);
      fclose(tempFile);
      return 1;
    }
    close(oldStderrFd);
    fseek(tempFile, 0, SEEK_SET);
    std::ostringstream buffer;
    char ch;
    while ((ch = fgetc(tempFile)) != EOF) {
      buffer << ch;
    }
    fclose(tempFile);
    std::string output = buffer.str();
    std::cout << "Captured stderr: " << output << std::endl;
    if (ret != 0) {
      return 1;
    }
    if (output.find("error") != std::string::npos) {
      return 1;
    }
    return 0;
  }
}
