#include "operation.h"

#include <openssl/err.h>
#include <openssl/x509.h>
#include <stdlib.h>
#include <string.h>

int target_init(void) { return OPENSSL_init_crypto(OPENSSL_INIT_NO_LOAD_CONFIG, NULL) == 1 ? 0 : -1; }
const char *target_operation(void) { return "certificate.x509.parse.der"; }
uint8_t target_schema(void) { return 2; }
void target_cleanup(void) {}

void target_execute(const uint8_t *input, size_t length,
                    struct diffray_operation_result *result) {
  const unsigned char *cursor = input;
  ERR_clear_error();
  X509 *certificate = d2i_X509(NULL, &cursor, (long)length);
  if (certificate == NULL || cursor != input + length) {
    X509_free(certificate);
    ERR_clear_error();
    result->class_id = DIFFRAY_RESULT_REJECTED;
    return;
  }
  int encoded_length = i2d_X509(certificate, NULL);
  if (encoded_length <= 0) {
    X509_free(certificate);
    ERR_clear_error();
    result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "OpenSSL DER length query failed");
    return;
  }
  if ((size_t)encoded_length > DIFFRAY_INPUT_LIMIT) {
    X509_free(certificate);
    ERR_clear_error();
    result->class_id = DIFFRAY_RESULT_RESOURCE_LIMIT;
    return;
  }
  unsigned char *encoded = malloc((size_t)encoded_length);
  unsigned char *output = encoded;
  if (encoded == NULL || i2d_X509(certificate, &output) != encoded_length) {
    free(encoded);
    X509_free(certificate);
    ERR_clear_error();
    result->class_id = DIFFRAY_RESULT_ADAPTER_ERROR;
    strcpy(result->diagnostic, "OpenSSL DER export failed");
    return;
  }
  X509_free(certificate);
  ERR_clear_error();
  result->class_id = DIFFRAY_RESULT_ACCEPTED;
  result->normalized = encoded;
  result->normalized_len = (size_t)encoded_length;
}
