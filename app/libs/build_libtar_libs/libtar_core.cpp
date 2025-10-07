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
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
#include <vector>

// 递归删除目录及其内容
void remove_directory(const char *path) {
  DIR *dir = opendir(path);
  if (dir == nullptr) {
    perror("opendir");
    return;
  }

  struct dirent *entry;
  while ((entry = readdir(dir)) != nullptr) {
    if (strcmp(entry->d_name, ".") == 0 || strcmp(entry->d_name, "..") == 0) {
      continue;
    }

    std::string full_path = std::string(path) + "/" + entry->d_name;
    if (entry->d_type == DT_DIR) {
      // 如果是目录，递归删除
      remove_directory(full_path.c_str());
    } else {
      // 如果是文件，直接删除
      if (unlink(full_path.c_str()) != 0) {
        perror("unlink");
      }
    }
  }

  closedir(dir);

  // 删除空目录
  if (rmdir(path) != 0) {
    perror("rmdir");
  }
}

extern "C" int tar_parse(const uint8_t *data, size_t size) {
  TAR *ptar_handle = NULL;
  int iret = -1;
  int ret = 1;

  // 创建一个临时文件，用于存储模糊测试输入
  char tmp_filename[] = "/app/fuzz_tar_XXXXXX";
  int fd = mkstemp(tmp_filename);
  if (fd < 0) {
    perror("mkstemp");
    return 1;
  }

  // 将模糊测试输入写入临时文件
  if (write(fd, data, size) != (ssize_t)size) {
    perror("write");
    close(fd);
    unlink(tmp_filename);
    return 1;
  }
  close(fd);

  // 创建一个临时目录，用于解压文件
  char tmp_dir[] = "/app/fuzz_tar_extract_XXXXXX";
  mkdtemp(tmp_dir);

  // 打开 tar 文件
  iret = tar_open(&ptar_handle, tmp_filename, NULL, O_RDONLY, 0644, TAR_GNU);
  // TAR_IGNORE_CRC
  if (iret != 0) {
    // 打开失败，清理临时文件和目录
    unlink(tmp_filename);
    remove_directory(tmp_dir);
    return 1;
  }

  // 尝试解压所有文件
  if (tar_extract_all(ptar_handle, tmp_dir) == 0) {
    // 如果解压成功，打印 success
    // printf("success\n");
    ret = 0;
  }

  // 关闭 tar 句柄并清理资源
  tar_close(ptar_handle);
  unlink(tmp_filename);
  remove_directory(tmp_dir);

  return ret;
}