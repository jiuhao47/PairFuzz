#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/stat.h>
#include <unistd.h>

#include "7z_core.c"

int main(int argc, char **argv) {
  if (argc != 2) {
    fprintf(stderr, "Usage: %s <xz-file>\n", argv[0]);
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
  void *map = mmap(NULL, size, PROT_READ, MAP_PRIVATE, fd, 0);
  if (map == MAP_FAILED) {
    fprintf(stderr, "mmap failed: %s\n", strerror(errno));
    close(fd);
    return 6;
  }

  const uint8_t *data = (const uint8_t *)map;
  int rc = cli_scanxz(data, size);

  // Print result so callers can parse it. Format matches other harnesses.
  printf("[RV_FLAG] ret = %d\n", rc);

  // Cleanup
  munmap(map, size);
  close(fd);

  return (rc == 0) ? 0 : 1;
}
