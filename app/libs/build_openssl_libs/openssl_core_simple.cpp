/*
 * Copyright 2016-2021 The OpenSSL Project Authors. All Rights Reserved.
 *
 * Licensed under the Apache License 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 * https://www.openssl.org/source/license.html
 * or in the file LICENSE in the source distribution.
 */

#include <openssl/bio.h>
#include <openssl/err.h>
#include <openssl/rand.h>
#include <openssl/x509.h>
#define ACCEPT 0
#define REJECT 1
#define PRINT_ERR 2
#define FAIL_TO_CREATE_BIO 3

extern "C" int verify_cert_mem_openssl(const uint8_t *data_cert,
                                       uint32_t size_cert) {
  const unsigned char *p = data_cert;
  unsigned char *der = NULL;

  X509 *x509 = d2i_X509(NULL, &p, size_cert);
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