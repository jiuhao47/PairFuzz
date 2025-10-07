
#include <assert.h>
#include <dirent.h>
#include <openssl/bio.h>
#include <openssl/crypto.h>
#include <openssl/err.h>
#include <openssl/pem.h>
#include <openssl/ssl.h>
#include <openssl/x509.h>
#include <openssl/x509_vfy.h>
#include <openssl/x509v3.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <string>

#define compositize_ret_val(raw_ret, depth, err)                               \
  ((raw_ret) | (depth << 4) | (err << 12))

// Note: paths must be provided via environment variables
// DIFFRAY_OPENSSL_CAPATH and/or DIFFRAY_OPENSSL_CERTDIR. If neither is set
// the process will abort. This enforces configurable certificate locations
// for fuzzing runs.

using namespace std;

void addtostore(X509_STORE *store) {
  // Require the single environment variable DIFFRAY_OPENSSL_CERTDIR to be set.
  const char *env_certdir = getenv("DIFFRAY_OPENSSL_CERTDIR");

  if (!env_certdir) {
    fprintf(stderr,
            "FATAL: must set DIFFRAY_OPENSSL_CERTDIR environment variable\n");
    abort();
  }

  // Helper to iterate a directory and load regular files into the store.
  auto load_from_dir = [&](const char *dirpath) {
    if (!dirpath)
      return;
    DIR *d = opendir(dirpath);
    if (!d)
      return;
    struct dirent *entry;
    std::string base = dirpath;
    if (!base.empty() && base.back() != '/')
      base.push_back('/');
    while ((entry = readdir(d)) != NULL) {
      if (entry->d_type == DT_REG) {
        std::string filePath = base + entry->d_name;
        if (filePath.find("xxx") == std::string::npos) { // 过滤掉xxx
          X509_STORE_load_locations(store, filePath.c_str(), 0);
        }
      }
    }
    closedir(d);
  };

  // Load certs from the provided directory
  load_from_dir(env_certdir);
}

struct GlobalState {
  GlobalState() : caStore(InitCaStore()) { OpenSSL_add_all_algorithms(); }

  ~GlobalState() {
    X509_STORE_free(caStore);
    EVP_cleanup();
  }

  X509_STORE *InitCaStore() {
    X509_STORE *store = NULL;
    int rc;
    store = X509_STORE_new();
    assert(store != NULL);
    addtostore(store);
    return store;
  }

  // Global CA store
  X509_STORE *caStore;
};

extern "C" int verify_cert_mem_new(const uint8_t *data_cert,
                                   uint32_t size_cert) {
  static GlobalState g_state;
  int composite_ret = -1;
  int raw_ret = -1;
  int err = -1;
  int depth = 0;
  X509 *leaf = NULL;
  X509_STORE_CTX *sctx = NULL;
  BIO *bio;

  ERR_clear_error();

  if ((bio = BIO_new_mem_buf((uint8_t *)data_cert, size_cert)) == NULL) {
    printf("ERROR: X.509 create BIO failed\n");
    return -1;
  }

  leaf = d2i_X509_bio(bio, NULL);

  // It's possible after mutation, we cannot even parse the certificate.
  if (leaf == NULL) {
    raw_ret = -2;
    composite_ret = -2;
    err = -2;
    printf("ERROR: X.509 leaf node convert failed\n");
    goto exit1;
  }

  if ((sctx = X509_STORE_CTX_new()) == NULL) {
    printf("ERROR: X.509 store context allocation failed\n");
    goto exit;
  }

  if (!X509_STORE_CTX_init(sctx, g_state.caStore, leaf, 0)) {
    printf("ERROR: X.509 init store context failed\n");
    goto exit;
  }

exit:
  raw_ret = X509_verify_cert(sctx);
  err = X509_STORE_CTX_get_error(sctx);
  depth = X509_STORE_CTX_get_error_depth(sctx);

exit1:
  composite_ret = compositize_ret_val(raw_ret, depth, err);
  X509_STORE_CTX_free(sctx);
  X509_free(leaf);
  BIO_free(bio);

  if (composite_ret == 1)
    return 0;
  // printf("raw ret is %d\n",raw_ret);
  return composite_ret;
}

extern "C" int verify_cert_mem_openssl(const uint8_t *data_cert,
                                       uint32_t size_cert) {
  return verify_cert_mem_new(data_cert, size_cert);
}
