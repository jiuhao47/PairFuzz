#include <archive.h>
#include <archive_entry.h>
#include <stdint.h>
#include <stddef.h>
#include <stdio.h>

#include <string>
#include <vector>
#include <functional> // 添加此行以包含 std::function
#include <stdio.h>
#include <assert.h>
#include <stdint.h>
#include <iostream>
#include <fstream>

#include <string>
#include <vector>
#include <stdlib.h>
#include <string.h>
#include <dirent.h>

#include <stdarg.h>
#include <stddef.h>
#include <ctype.h>


extern "C" int archive_parse(const uint8_t *data, size_t size) {
    struct archive *a;
    struct archive_entry *entry;
    int r;
    int ret =1;
    // 创建一个用于读取内存数据的 archive 对象
    a = archive_read_new();
    if (a == NULL) {
        return 1;
    }

    // 启用对 tar 格式的支持
    archive_read_support_format_tar(a);

    // 将输入数据绑定到 archive 对象
    r = archive_read_open_memory(a, data, size);
    if (r != ARCHIVE_OK) {
        archive_read_free(a);
        return 1;
    }

    // 遍历 tar 文件中的每个条目
    while ((r = archive_read_next_header(a, &entry)) == ARCHIVE_OK) {
        // 打印条目的路径名（可选）
        const char *pathname = archive_entry_pathname(entry);
        if (pathname) {
            // printf("Entry: %s\n", pathname);
        }

        // 读取条目的数据（可选）
        const void *buff;
        size_t buff_size;
        int64_t offset;
        while (archive_read_data_block(a, &buff, &buff_size, &offset) == ARCHIVE_OK) {
            // 数据块已被读取，可以在此处处理数据
        }
    }

    // 如果成功解析所有条目，则打印 "success"
    if (r == ARCHIVE_EOF) {
        // printf("success\n");
        ret = 0;
    }

    // 释放资源
    archive_read_free(a);
    return ret;
}

extern "C" int diffRayTestOneInput_1(const uint8_t *data, size_t size) {
  return archive_parse(data, size);
}