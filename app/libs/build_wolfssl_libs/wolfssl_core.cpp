#define OPENSSL_EXTRA 1
#define WC_NO_HARDEN 1
#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <wolfssl/options.h>
#include <wolfssl/ssl.h>

//  ../configure --enable-opensslextra

#define compositize_ret_val(raw_ret, depth, err)                               \
  ((raw_ret) | (depth << 4) | (err << 12))

// extern "C" int verify_wolfssl(const unsigned char *cert_buffer,
//                               size_t cert_size);
// because we use "#define OPENSSL_EXTRA 1", there is no need to define the
// following interface extern "C" int
// wolfSSL_X509_STORE_load_locations(WOLFSSL_X509_STORE *str, const char *file,
// const char *dir);

long load_der_cert(const char *filename, unsigned char **buffer);

// CA path is obtained from environment. Prefer DIFFRAY_WOLFSSL_CERTDIR, then
// fall back to DIFFRAY_OPENSSL_CERTDIR for compatibility.

struct GlobalState {
  GlobalState() : caStore(InitCaStore()) { wolfSSL_add_all_algorithms(); }

  ~GlobalState() {
    X509_STORE_free(caStore);
    wolfSSL_Cleanup();
  }

  X509_STORE *InitCaStore() {
    WOLFSSL_X509_STORE *store = NULL;
    store = wolfSSL_X509_STORE_new();
    assert(store != NULL);

    const char *env_capath = getenv("DIFFRAY_WOLFSSL_CERTDIR");
    if (!env_capath)
      env_capath = getenv("DIFFRAY_OPENSSL_CERTDIR");

    if (!env_capath) {
      fprintf(stderr, "FATAL: must set DIFFRAY_WOLFSSL_CERTDIR or "
                      "DIFFRAY_OPENSSL_CERTDIR environment variable\n");
      abort();
    }

    int ret = wolfSSL_X509_STORE_load_locations(store, 0, env_capath);
    if (ret != WOLFSSL_SUCCESS) {
      fprintf(stderr, "ERROR: failed to load CA certificates from %s\n",
              env_capath);
    }
    assert(ret == WOLFSSL_SUCCESS);
    return store; // Loading CA certificates failed
  }

  // Global CA store
  WOLFSSL_X509_STORE *caStore;
};

extern "C" int verify_wolfssl(const unsigned char *cert_buffer,
                              size_t cert_size) {
  // int ret = wolfSSL_Debugging_ON();
  static GlobalState g_state;
  WOLFSSL_X509 *x509 = NULL;
  WOLFSSL_X509_STORE_CTX *ctx = NULL;

  wolfSSL_ERR_clear_error();

  // Initialize WolfSSL library
  if (wolfSSL_Init() != WOLFSSL_SUCCESS) {
    // fprintf(stderr, "Error initializing WolfSSL library\n");
    return 2; // Initialization failed
  }

  // This function will create and init the x509 object
  if (wolfSSL_X509_d2i(&x509, cert_buffer, cert_size) == NULL) {
    // fprintf(stderr, "Error loading DER-encoded certificate from memory\n");
    wolfSSL_X509_free(x509);
    wolfSSL_Cleanup();
    return 3; // Certificate loading failed
  }

  // Create a new X509_STORE_CTX object
  if ((ctx = wolfSSL_X509_STORE_CTX_new()) == NULL) {
    // fprintf(stderr, "Error creating X509_STORE_CTX object\n");
    wolfSSL_X509_free(x509);
    // sk_X509_pop_free(untrusted, X509_free);
    wolfSSL_Cleanup();
    return 4; // Object creation failed
  }

  // // Initialize the X509_STORE_CTX with the store and the certificate
  if (wolfSSL_X509_STORE_CTX_init(ctx, g_state.caStore, x509, 0) !=
      WOLFSSL_SUCCESS) {
    // fprintf(stderr, "Error initializing X509_STORE_CTX\n");
    wolfSSL_X509_STORE_CTX_free(ctx);
    wolfSSL_X509_free(x509);
    wolfSSL_Cleanup();
    return 5; // Initialization failed
  }

  // Perform certificate verification
  int verification_result = wolfSSL_X509_verify_cert(ctx);
  int composite_ret = -1;
  int raw_ret = -1;
  int err = -1;
  int depth = 0;

  // return zero, means that error occured
  if (verification_result != WOLFSSL_SUCCESS) {
    raw_ret = verification_result;
    err = wolfSSL_X509_STORE_CTX_get_error(ctx);
    depth = wolfSSL_X509_STORE_CTX_get_error_depth(ctx);
    composite_ret = compositize_ret_val(raw_ret, depth, err);
    // fprintf(stderr, "Failed, error code:%d, cause:%s\n", composite_ret,
    // wolfSSL_X509_verify_cert_error_string(raw_ret));
  } else {
    composite_ret = 0;
    // fprintf(stderr, "Certificate verification succeeded:%d\n",
    // composite_ret);
  }

  // Clean up
  ctx->store = NULL;
  wolfSSL_X509_STORE_CTX_free(ctx); // already frees the ctx->store
  wolfSSL_X509_free(x509);
  wolfSSL_Cleanup();

  return composite_ret;
}

long load_der_cert(const char *filename, unsigned char **buffer) {
  FILE *file = fopen(filename, "rb");
  if (!file) {
    perror("Error opening certificate file");
    return 0;
  }
  fseek(file, 0, SEEK_END);
  long file_size = ftell(file);
  fseek(file, 0, SEEK_SET);

  *buffer = (unsigned char *)malloc(file_size);
  fread(*buffer, 1, file_size, file);
  fclose(file);
  return file_size;
}
