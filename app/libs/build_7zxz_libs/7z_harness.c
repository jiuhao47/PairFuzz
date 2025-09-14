#include "7zTypes.h"
#include "Alloc.h"
#include "Xz.h"
#include "XzCrc64.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>


extern int cli_scanxz( const unsigned char *data, unsigned int size );
extern int diffRayTestOneInput_2(const uint8_t *data, size_t size) {

  return cli_scanxz(data,size);
}