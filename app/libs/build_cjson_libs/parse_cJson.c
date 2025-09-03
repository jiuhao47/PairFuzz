#include <stdint.h>
#include <stddef.h>
#include <stdio.h>  // For printf
#include <stdlib.h>
#include <string.h>
#include "cJSON.h"

extern int cjson_parse(const uint8_t *Data, size_t Size) {

    if (Size==0) {
        return 100;
    }

    unsigned char *buf = (unsigned char *) malloc(Size+1);
    buf[Size] = 0;
    memcpy(buf, Data, Size);

    int ret = 1;
    cJSON *root = NULL;
    root = cJSON_Parse((char *)buf);

    if (root !=NULL &&  root->type == 64 ) // json object
    {
        ret = 0;
        cJSON_Delete(root);
    }else{
        if (root!=NULL) {
            cJSON_Delete(root);
        }
    }

    free(buf);

    return ret;
}