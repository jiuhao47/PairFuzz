#ifndef DIFFRAY_X509_FACTS_H
#define DIFFRAY_X509_FACTS_H

#include "semantic.h"

#include <stdio.h>
#include <string.h>
#include <time.h>

static int x509_json_string(char *output, size_t capacity, const char *value,
                            size_t limit) {
  if (value == NULL) return snprintf(output, capacity, "null");
  size_t used = 0;
  if (capacity < 3) return -1;
  output[used++] = '"';
  for (const unsigned char *cursor = (const unsigned char *)value;
       *cursor != 0 && (size_t)(cursor - (const unsigned char *)value) < limit;
       ++cursor) {
    const char *escape = NULL;
    char encoded[7];
    if (*cursor == '"') escape = "\\\"";
    else if (*cursor == '\\') escape = "\\\\";
    else if (*cursor == '\n') escape = "\\n";
    else if (*cursor == '\r') escape = "\\r";
    else if (*cursor == '\t') escape = "\\t";
    else if (*cursor < 0x20 || *cursor >= 0x7f) {
      (void)snprintf(encoded, sizeof(encoded), "\\u%04x", *cursor);
      escape = encoded;
    }
    size_t count = escape == NULL ? 1 : strlen(escape);
    if (used + count + 2 > capacity) return -1;
    if (escape == NULL) output[used++] = (char)*cursor;
    else {
      memcpy(output + used, escape, count);
      used += count;
    }
  }
  output[used++] = '"';
  output[used] = 0;
  return (int)used;
}

static void x509_time_text(const ASN1_TIME *value, char output[32]) {
  struct tm calendar;
  output[0] = 0;
  if (value != NULL && ASN1_TIME_to_tm(value, &calendar) == 1) {
    (void)snprintf(output, 32, "%04d-%02d-%02dT%02d:%02d:%02dZ",
                   calendar.tm_year + 1900, calendar.tm_mon + 1,
                   calendar.tm_mday, calendar.tm_hour, calendar.tm_min,
                   calendar.tm_sec);
  }
}

static void x509_oid_text(const ASN1_OBJECT *object, char output[64]) {
  output[0] = 0;
  if (object != NULL && OBJ_obj2txt(output, 64, object, 1) <= 0) output[0] = 0;
}

static int x509_write_facts(X509 *certificate,
                            char output[DIFFRAY_FACTS_LIMIT]) {
  char serial[65] = {0};
  char subject[96] = {0};
  char issuer[96] = {0};
  char not_before[32] = {0};
  char not_after[32] = {0};
  char public_key_oid[64] = {0};
  char signature_oid[64] = {0};
  char serial_json[80], subject_json[128], issuer_json[128];
  char before_json[48], after_json[48], public_json[80], signature_json[80];

  const ASN1_INTEGER *serial_value = X509_get0_serialNumber(certificate);
  int serial_length =
      serial_value == NULL ? 0 : ASN1_STRING_length(serial_value);
  const unsigned char *serial_bytes =
      serial_value == NULL ? NULL : ASN1_STRING_get0_data(serial_value);
  if (serial_length > 0 && serial_length <= 32 && serial_bytes != NULL) {
    for (int index = 0; index < serial_length; ++index) {
      (void)snprintf(serial + index * 2, sizeof(serial) - (size_t)index * 2,
                     "%02x", serial_bytes[index]);
    }
  }
  (void)X509_NAME_oneline(X509_get_subject_name(certificate), subject,
                          (int)sizeof(subject));
  (void)X509_NAME_oneline(X509_get_issuer_name(certificate), issuer,
                          (int)sizeof(issuer));
  x509_time_text(X509_get0_notBefore(certificate), not_before);
  x509_time_text(X509_get0_notAfter(certificate), not_after);

  EVP_PKEY *public_key = X509_get0_pubkey(certificate);
  x509_oid_text(public_key == NULL ? NULL : OBJ_nid2obj(EVP_PKEY_base_id(public_key)),
                public_key_oid);
  x509_oid_text(OBJ_nid2obj(X509_get_signature_nid(certificate)), signature_oid);

  if (x509_json_string(serial_json, sizeof(serial_json), serial[0] ? serial : NULL, 64) < 0 ||
      x509_json_string(subject_json, sizeof(subject_json), subject[0] ? subject : NULL, 72) < 0 ||
      x509_json_string(issuer_json, sizeof(issuer_json), issuer[0] ? issuer : NULL, 72) < 0 ||
      x509_json_string(before_json, sizeof(before_json), not_before[0] ? not_before : NULL, 31) < 0 ||
      x509_json_string(after_json, sizeof(after_json), not_after[0] ? not_after : NULL, 31) < 0 ||
      x509_json_string(public_json, sizeof(public_json), public_key_oid[0] ? public_key_oid : NULL, 63) < 0 ||
      x509_json_string(signature_json, sizeof(signature_json), signature_oid[0] ? signature_oid : NULL, 63) < 0) {
    return -1;
  }
  int written = snprintf(
      output, DIFFRAY_FACTS_LIMIT,
      "{\"version\":%ld,\"serial_hex\":%s,\"subject\":%s,\"issuer\":%s,"
      "\"not_before\":%s,\"not_after\":%s,\"public_key_algorithm_oid\":%s,"
      "\"signature_algorithm_oid\":%s}",
      X509_get_version(certificate) + 1, serial_json, subject_json, issuer_json,
      before_json, after_json, public_json, signature_json);
  return written > 0 && written < (int)DIFFRAY_FACTS_LIMIT ? 0 : -1;
}

#endif
