#include <wolfssl/ssl.h>

#include <wolfssl/openssl/bio.h>
#include <wolfssl/openssl/evp.h>
#include <wolfssl/openssl/x509.h>

#define ACCEPT 0
#define REJECT 1
#define PRINT_ERR 2
#define FAIL_TO_CREATE_BIO 3

extern "C" int verify_wolfssl(const unsigned char *cert_buffer,
                              size_t cert_size) {
  const unsigned char *p = cert_buffer;
  unsigned char *der = NULL;

  X509 *x509 = d2i_X509(NULL, &p, cert_size);
  if (x509 != NULL) {
    BIO *bio = BIO_new(BIO_s_mem());
    if (bio == NULL) {
      X509_free(x509);
      return FAIL_TO_CREATE_BIO;
    }
    /* This will load and print the public key as well as extensions */
    if (X509_print(bio, x509) == 0) {
      BIO_free(bio);
      X509_free(x509);
      return PRINT_ERR;
    }
    BIO_free(bio);

    // X509_issuer_and_serial_hash(x509);

    i2d_X509(x509, &der);
    OPENSSL_free(der);

    X509_free(x509);
  } else {
    return REJECT;
  }
  ERR_clear_error();
  return ACCEPT;
}
