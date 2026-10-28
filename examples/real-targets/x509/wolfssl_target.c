#include "operation.h"

#include <limits.h>
#include <stdlib.h>
#include <string.h>
#include <wolfssl/options.h>
#include <wolfssl/openssl/evp.h>
#include <wolfssl/openssl/objects.h>
#include <wolfssl/openssl/ssl.h>

#include "x509_facts.h"

int target_init(void) { return wolfSSL_Init() == WOLFSSL_SUCCESS ? 0 : -1; }
const char *target_operation(void) { return "certificate.x509.parse.der"; }
uint8_t target_schema(void) { return 2; }
void target_cleanup(void) { wolfSSL_Cleanup(); }

void target_execute(const uint8_t *input, size_t length,
                    struct diffray_operation_result *result) {
  if (length > INT_MAX) {
    result->class_id = DIFFRAY_RESULT_RESOURCE_LIMIT;
    return;
  }
  const unsigned char *cursor = input;
  WOLFSSL_X509 *certificate = wolfSSL_d2i_X509(NULL, &cursor, (int)length);
  if (certificate == NULL || cursor != input + length) {
    wolfSSL_X509_free(certificate);
    result->class_id = DIFFRAY_RESULT_REJECTED;
    return;
  }
  int encoded_length = wolfSSL_i2d_X509(certificate, NULL);
  if (encoded_length <= 0) {
    wolfSSL_X509_free(certificate);
    result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "wolfSSL DER length query failed");
    return;
  }
  if ((size_t)encoded_length > DIFFRAY_INPUT_LIMIT) {
    wolfSSL_X509_free(certificate);
    result->class_id = DIFFRAY_RESULT_RESOURCE_LIMIT;
    return;
  }
  unsigned char *encoded = malloc((size_t)encoded_length);
  unsigned char *output = encoded;
  if (encoded == NULL || wolfSSL_i2d_X509(certificate, &output) != encoded_length) {
    free(encoded);
    wolfSSL_X509_free(certificate);
    result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "wolfSSL DER export failed");
    return;
  }
  wolfSSL_X509_free(certificate);
  result->class_id = DIFFRAY_RESULT_ACCEPTED;
  result->normalized = encoded;
  result->normalized_len = (size_t)encoded_length;
}

int target_replay_facts(const uint8_t *input, size_t length,
                        char output[DIFFRAY_FACTS_LIMIT]) {
  if (length > INT_MAX) return -1;
  const unsigned char *cursor = input;
  WOLFSSL_X509 *certificate = wolfSSL_d2i_X509(NULL, &cursor, (int)length);
  if (certificate == NULL || cursor != input + length) {
    wolfSSL_X509_free(certificate);
    return -1;
  }
  int status = x509_write_facts(certificate, output);
  wolfSSL_X509_free(certificate);
  return status;
}
