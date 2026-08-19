#include <criterion/criterion.h>
#include <stddef.h>
#include <stdlib.h>

int zenos_atoi(const char *str);
long zenos_atol(const char *str);

#define ARRAY_LEN(array) (sizeof(array) / sizeof((array)[0]))

Test(stdlib, atoi_matches_host) {
  const char *inputs[] = {"",     "0",          "123",         "-456",
                          "+789", "  42",       "\t\n123",     "123abc",
                          "   ",  "2147483647", "-2147483647", "-0"};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    cr_expect_eq(zenos_atoi(inputs[i]), atoi(inputs[i]),
                 "atoi differed for input %zu (%s)", i, inputs[i]);
  }
}

Test(stdlib, atol_matches_host) {
  const char *inputs[] = {"",    "0",          "123456789",  "-987654321",
                          "+42", "  12345",    "123abc",     "\t-7",
                          "-0",  "2147483647", "-2147483647"};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    cr_expect_eq(zenos_atol(inputs[i]), atol(inputs[i]),
                 "atol differed for input %zu (%s)", i, inputs[i]);
  }
}
