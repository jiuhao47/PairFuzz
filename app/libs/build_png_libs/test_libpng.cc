// Simple test driver that includes libpng_core.cc and calls libpng_parse on a
// file
#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

// Include the implementation directly so we can call libpng_parse
#include "libpng_core.cc"

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "Usage: %s <png-file>\n", argv[0]);
    return 2;
  }

  const char *path = argv[1];
  int fd = open(path, O_RDONLY);
  if (fd == -1) {
    fprintf(stderr, "open(%s) failed: %s\n", path, strerror(errno));
    return 3;
  }

  struct stat st;
  if (fstat(fd, &st) == -1) {
    fprintf(stderr, "fstat failed: %s\n", strerror(errno));
    close(fd);
    return 4;
  }

  if (st.st_size == 0) {
    fprintf(stderr, "file is empty\n");
    close(fd);
    return 5;
  }

  size_t size = (size_t)st.st_size;
  void *map = mmap(nullptr, size, PROT_READ, MAP_PRIVATE, fd, 0);
  if (map == MAP_FAILED) {
    fprintf(stderr, "mmap failed: %s\n", strerror(errno));
    close(fd);
    return 6;
  }

  const uint8_t *data = static_cast<const uint8_t *>(map);
  int rc = libpng_parse(data, size);

  // Print result to stdout in the requested format so caller (python) can parse
  // it. Format: [RV_FLAG] ret = %d\n
  printf("[RV_FLAG] ret = %d\n", rc);

  // Cleanup
  munmap(map, size);
  close(fd);

  // Use exit code only as success/failure signal: 0 if libpng_parse returned 0,
  // otherwise 1. The full rc is available in stdout above.
  return (rc == 0) ? 0 : 1;
}
