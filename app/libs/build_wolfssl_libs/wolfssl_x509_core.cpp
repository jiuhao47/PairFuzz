/* Adapted to wolfSSL: use WOLFSSL_* APIs so this file links when building
  against wolfSSL. Mirrors the composite return encoding used elsewhere. */
#define OPENSSL_EXTRA 1
#define WC_NO_HARDEN 1
#include <assert.h>
#include <fcntl.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/stat.h>
#include <sys/types.h>
#include <unistd.h>
#include <wolfssl/openssl/err.h>
#include <wolfssl/openssl/pem.h>
#include <wolfssl/openssl/x509.h>
#include <wolfssl/openssl/x509_vfy.h>
#include <wolfssl/openssl/x509v3.h>
#include <wolfssl/options.h>
#include <wolfssl/ssl.h>

#define compositize_ret_val(raw_ret, depth, err)                               \
  ((raw_ret) | (depth << 4) | (err << 12))

extern "C" {

static ssize_t parse_der_total_length_fn(const unsigned char *p, size_t max) {
  if (max < 2)
    return -1;
  if (p[0] != 0x30) // expecting SEQUENCE
    return -1;
  unsigned char lbyte = p[1];
  size_t header_len = 2;
  size_t len = 0;
  if ((lbyte & 0x80) == 0) {
    len = lbyte;
  } else {
    int n = lbyte & 0x7F;
    if (n <= 0 || n > 4)
      return -1;
    if (max < 2 + (size_t)n)
      return -1;
    len = 0;
    for (int i = 0; i < n; ++i) {
      len = (len << 8) | p[2 + i];
    }
    header_len = 2 + n;
  }
  if (header_len + len > max)
    return -1;
  return (ssize_t)(header_len + len);
}

int verify_x509_composite(const uint8_t *data, uint32_t size) {
  int composite_ret = -1;
  int raw_ret = -1;
  int err = -1;
  int depth = 0;

  // declare important pointer variables early so goto cleanup doesn't jump
  // over their initializations
  X509 *x509 = NULL;
  X509_STORE *store = NULL;
  X509_STORE_CTX *ctx = NULL;

  /* Declare parsing/offset variables early so a goto to cleanup does not
    jump past their initializations (which is ill-formed in C++). */
  ssize_t len1 = -1;
  size_t offset = 0;

  ERR_clear_error();

  if (wolfSSL_Init() != 1) {
    raw_ret = -10;
    err = -10;
    composite_ret = compositize_ret_val(raw_ret, depth, err);
    return composite_ret;
  }

  if (wolfSSL_X509_d2i(&x509, data, size) == NULL) {
    X509_free(x509);
    wolfSSL_Cleanup();
    raw_ret = -2;
    err = -2;
    composite_ret = compositize_ret_val(raw_ret, depth, err);
    return composite_ret;
  }
  /* Determine CA path early so we don't declare new variables after a goto. */
  const char *env_capath = getenv("DIFFRAY_WOLFSSL_CERTDIR");
  if (!env_capath)
    env_capath = getenv("DIFFRAY_OPENSSL_CERTDIR");

  // use static helper parse_der_total_length_fn defined above

  store = X509_STORE_new();
  if (store == NULL) {
    raw_ret = -3;
    err = -3;
    composite_ret = compositize_ret_val(raw_ret, depth, err);
    goto cleanup;
  }
  if (env_capath) {
    X509_STORE_load_locations(store, 0, env_capath);
  }

  // Attempt to locate a second DER-encoded certificate in the input by
  // parsing ASN.1 SEQUENCE length headers. If found, write exactly that DER
  // to a temporary file and load it into the store as a per-input trust anchor.

  len1 = parse_der_total_length_fn((const unsigned char *)data, size);
  if (len1 > 0) {
    offset = (size_t)len1;
  } else {
    /* fallback: if we couldn't detect length, we still try to proceed without a
       second cert */
    offset = 0;
  }

  if (offset < size) {
    // try parse second cert at offset
    ssize_t len2 = parse_der_total_length_fn(
        (const unsigned char *)data + offset, size - offset);
    if (len2 <= 0) {
      // try scanning for possible SEQUENCE header
      for (size_t i = offset + 1; i + 4 < size; ++i) {
        if (data[i] == 0x30) {
          ssize_t cand = parse_der_total_length_fn(
              (const unsigned char *)data + i, size - i);
          if (cand > 0) {
            offset = i;
            len2 = cand;
            break;
          }
        }
      }
    }

    if (len2 > 0 && (size_t)len2 <= size - offset) {
      // write DER bytes [offset, offset+len2) to a temporary file
      char tmpl[] = "/tmp/diffray_cert_XXXXXX";
      int fd = mkstemp(tmpl);
      if (fd != -1) {
        ssize_t wrote = write(fd, data + offset, (size_t)len2);
        (void)wrote;
        close(fd);
        // load this file as a trust anchor
        wolfSSL_X509_STORE_load_locations(store, tmpl, NULL);
        unlink(tmpl);
      }
    }
  }

  ctx = X509_STORE_CTX_new();
  if (ctx == NULL) {
    raw_ret = -4;
    err = -4;
    composite_ret = compositize_ret_val(raw_ret, depth, err);
    goto cleanup;
  }

  if (X509_STORE_CTX_init(ctx, store, x509, 0) != 1) {
    raw_ret = -5;
    err = X509_STORE_CTX_get_error(ctx);
    depth = X509_STORE_CTX_get_error_depth(ctx);
    composite_ret = compositize_ret_val(raw_ret, depth, err);
    goto cleanup;
  }

  raw_ret = X509_verify_cert(ctx);
  err = X509_STORE_CTX_get_error(ctx);
  depth = X509_STORE_CTX_get_error_depth(ctx);

  composite_ret = compositize_ret_val(raw_ret, depth, err);

cleanup:
  if (ctx) {
    ctx->store = NULL;
    X509_STORE_CTX_free(ctx);
  }
  if (store)
    X509_STORE_free(store);
  X509_free(x509);
  wolfSSL_Cleanup();
  ERR_clear_error();

  return composite_ret;
}

int verify_wolfssl(const uint8_t *data, uint32_t size) {
  return verify_x509_composite(data, size);
}

} // extern "C"
