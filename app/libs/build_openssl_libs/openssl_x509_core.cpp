#include <assert.h>
#include <openssl/bio.h>
#include <openssl/err.h>
#include <openssl/ocsp.h>
#include <openssl/pem.h>
#include <openssl/ssl.h>
#include <openssl/x509.h>
#include <openssl/x509_vfy.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define COMPOSITIZE_RET_VAL(raw_ret, depth, err)                               \
  ((raw_ret) | ((depth) << 4) | ((err) << 12))

extern "C" {

// Parse a memory buffer (possibly containing multiple ASN.1 objects), attempt
// to build a verification store (using a second certificate if present) and
// run X509_verify_cert on the first certificate. Returns a composite integer
// encoding of (raw_ret | depth<<4 | err<<12). Caller can decode these fields.
int verify_x509_composite(const uint8_t *data, uint32_t size) {
  int composite_ret = -1;
  int raw_ret = -1;
  int err = -1;
  int depth = 0;

  const unsigned char *p = data;
  long remain = (long)size;

  X509 *x509_1 = NULL;
  X509 *x509_2 = NULL;
  X509_STORE *store = NULL;
  X509_STORE_CTX *sctx = NULL;

  ERR_clear_error();

  // Parse first certificate (leaf)
  x509_1 = d2i_X509(NULL, &p, remain);
  if (x509_1 == NULL) {
    // Unable to parse a leaf certificate from input
    raw_ret = -2;
    err = -2;
    composite_ret = COMPOSITIZE_RET_VAL(raw_ret, depth, err);
    return composite_ret;
  }

  // Update remaining length
  remain = (long)size - (long)(p - data);

  // Try to parse a second certificate from remaining bytes (use as CA if
  // present)
  if (remain > 0) {
    x509_2 = d2i_X509(NULL, &p, remain);
    // x509_2 may be NULL if there is no second cert; that's acceptable
  }

  // Create a store and, if we have a second cert, add it as a trusted cert
  store = X509_STORE_new();
  if (store == NULL) {
    raw_ret = -3; // store allocation failed
    err = -3;
    composite_ret = COMPOSITIZE_RET_VAL(raw_ret, depth, err);
    goto cleanup;
  }

  if (x509_2 != NULL) {
    // Add x509_2 as a trusted cert to the store
    if (!X509_STORE_add_cert(store, x509_2)) {
      // If add fails, we still continue: verification may still run without it.
      // But record an error sentinel.
      // Note: X509_STORE_add_cert returns 0 on failure.
    }
  }

  sctx = X509_STORE_CTX_new();
  if (sctx == NULL) {
    raw_ret = -4; // ctx allocation failed
    err = -4;
    composite_ret = COMPOSITIZE_RET_VAL(raw_ret, depth, err);
    goto cleanup;
  }

  if (!X509_STORE_CTX_init(sctx, store, x509_1, NULL)) {
    raw_ret = -5; // ctx init failed
    err = X509_STORE_CTX_get_error(sctx);
    depth = X509_STORE_CTX_get_error_depth(sctx);
    composite_ret = COMPOSITIZE_RET_VAL(raw_ret, depth, err);
    goto cleanup;
  }

  // Perform verification
  raw_ret = X509_verify_cert(sctx);
  err = X509_STORE_CTX_get_error(sctx);
  depth = X509_STORE_CTX_get_error_depth(sctx);

  composite_ret = COMPOSITIZE_RET_VAL(raw_ret, depth, err);

cleanup:
  X509_STORE_CTX_free(sctx);
  X509_STORE_free(store);
  X509_free(x509_1);
  X509_free(x509_2);
  ERR_clear_error();

  return composite_ret;
}

// Alias with an openssl-like name
int verify_cert_mem_openssl(const uint8_t *data, uint32_t size) {
  return verify_x509_composite(data, size);
}

} // extern "C"
