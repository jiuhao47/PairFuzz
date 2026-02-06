// Simple test driver that uses the IM_core implementation and calls
// imagemagick_parse on a provided file.
//
// This file is intended to be a minimal compile/link test that mirrors the
// approach used for other libraries in this repository.

#include <errno.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

// Include the implementation directly so we can call imagemagick_parse
#include "IM_core.cpp"

int main(int argc, char **argv) {
  if (argc != 2 && argc != 3) {
    fprintf(stderr, "Usage: %s <file> [rv_mode]\n", argv[0]);
    fprintf(stderr,
            "  rv_mode: 0=bool(0/1), 1=hash(path), 2=hash(path+line)\n");
    return 2;
  }

  const char *path = argv[1];
  int rv_mode = 0;
  if (argc == 3) {
    char *end = nullptr;
    long parsed = strtol(argv[2], &end, 10);
    if (end == argv[2] || *end != '\0' || parsed < 0 || parsed > 2) {
      fprintf(stderr, "Invalid rv_mode: %s (expected 0/1/2)\n", argv[2]);
      return 2;
    }
    rv_mode = (int)parsed;
  }
  imagemagick_set_rv_mode(rv_mode);

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
  void *map = mmap(NULL, size, PROT_READ, MAP_PRIVATE, fd, 0);
  if (map == MAP_FAILED) {
    fprintf(stderr, "mmap failed: %s\n", strerror(errno));
    close(fd);
    return 6;
  }

  const uint8_t *data = (const uint8_t *)map;
  int rc = imagemagick_parse(data, size);

  // Print a small token so external callers can parse the return value.
  printf("[RV_FLAG] ret = %d\n", rc);

  // Cleanup
  munmap(map, size);
  close(fd);

  return (rc == 0) ? 0 : 1;
}
