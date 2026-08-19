#include <criterion/criterion.h>
#include <stddef.h>
#include <string.h>

size_t zenos_strlen(const char *s);
char *zenos_strcpy(char *dest, const char *src);
char *zenos_strncpy(char *dest, const char *src, size_t n);
int zenos_strcmp(const char *s1, const char *s2);
int zenos_strncmp(const char *s1, const char *s2, size_t n);
char *zenos_strcat(char *dest, const char *src);
char *zenos_strncat(char *dest, const char *src, size_t n);
char *zenos_strchr(const char *s, int c);
char *zenos_strrchr(const char *s, int c);
char *zenos_strstr(const char *haystack, const char *needle);
char *zenos_strtok(char *str, const char *delim);
void *zenos_memset(void *s, int c, size_t n);
int zenos_memcmp(const void *s1, const void *s2, size_t n);
void *zenos_memcpy(void *dest, const void *src, size_t n);
void *zenos_memmove(void *dest, const void *src, size_t n);

#define ARRAY_LEN(array) (sizeof(array) / sizeof((array)[0]))

static int comparison_sign(int result) { return (result > 0) - (result < 0); }

static ptrdiff_t pointer_offset(const void *base, size_t size,
                                const void *result) {
  if (result == NULL) {
    return -1;
  }

  const unsigned char *bytes = base;
  for (size_t i = 0; i < size; i++) {
    if (result == bytes + i) {
      return (ptrdiff_t)i;
    }
  }
  return -2;
}

Test(string, strlen_matches_host) {
  const char *inputs[] = {"", "x", "hello", "hello world",
                          "with\ttabs\nand\nlines"};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    cr_expect_eq(zenos_strlen(inputs[i]), strlen(inputs[i]),
                 "strlen differed for input %zu (%s)", i, inputs[i]);
  }
}

Test(string, strcpy_matches_host) {
  const char *inputs[] = {"", "x", "hello", "a longer string with spaces"};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    char actual[64] = {0};
    char expected[64] = {0};
    char *actual_result = zenos_strcpy(actual, inputs[i]);
    char *expected_result = strcpy(expected, inputs[i]);

    cr_expect_eq(pointer_offset(actual, sizeof(actual), actual_result),
                 pointer_offset(expected, sizeof(expected), expected_result));
    cr_expect_eq(memcmp(actual, expected, sizeof(actual)), 0,
                 "strcpy output differed for input %zu (%s)", i, inputs[i]);
  }
}

Test(string, strncpy_matches_host) {
  const struct {
    const char *source;
    size_t count;
  } inputs[] = {{"", 0}, {"", 8}, {"hi", 8}, {"hello", 5}, {"hello", 3}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    unsigned char actual[16];
    unsigned char expected[16];
    memset(actual, 0xa5, sizeof(actual));
    memset(expected, 0xa5, sizeof(expected));

    char *actual_result =
        zenos_strncpy((char *)actual, inputs[i].source, inputs[i].count);
    char *expected_result =
        strncpy((char *)expected, inputs[i].source, inputs[i].count);

    cr_expect_eq(pointer_offset(actual, sizeof(actual), actual_result),
                 pointer_offset(expected, sizeof(expected), expected_result));
    cr_expect_eq(memcmp(actual, expected, sizeof(actual)), 0,
                 "strncpy output differed for case %zu", i);
  }
}

Test(string, strcmp_matches_host) {
  const struct {
    const char *left;
    const char *right;
  } inputs[] = {{"", ""},       {"abc", "abc"}, {"abc", "abd"},
                {"abd", "abc"}, {"hello", ""},  {"hello", "hello world"}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    int actual = zenos_strcmp(inputs[i].left, inputs[i].right);
    int expected = strcmp(inputs[i].left, inputs[i].right);
    cr_expect_eq(comparison_sign(actual), comparison_sign(expected),
                 "strcmp differed for case %zu", i);
  }
}

Test(string, strncmp_matches_host) {
  const struct {
    const char *left;
    const char *right;
    size_t count;
  } inputs[] = {{"abc", "xyz", 0}, {"hello", "helps", 3}, {"hello", "hallo", 3},
                {"abc", "abd", 3}, {"abc", "abc", 8},     {"", "x", 1}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    int actual =
        zenos_strncmp(inputs[i].left, inputs[i].right, inputs[i].count);
    int expected = strncmp(inputs[i].left, inputs[i].right, inputs[i].count);
    cr_expect_eq(comparison_sign(actual), comparison_sign(expected),
                 "strncmp differed for case %zu", i);
  }
}

Test(string, strcat_matches_host) {
  const struct {
    const char *destination;
    const char *source;
  } inputs[] = {{"", ""}, {"", "hello"}, {"hello", ""}, {"hello", " world"}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    char actual[64] = {0};
    char expected[64] = {0};
    strcpy(actual, inputs[i].destination);
    strcpy(expected, inputs[i].destination);

    char *actual_result = zenos_strcat(actual, inputs[i].source);
    char *expected_result = strcat(expected, inputs[i].source);

    cr_expect_eq(pointer_offset(actual, sizeof(actual), actual_result),
                 pointer_offset(expected, sizeof(expected), expected_result));
    cr_expect_eq(memcmp(actual, expected, sizeof(actual)), 0,
                 "strcat output differed for case %zu", i);
  }
}

Test(string, strncat_matches_host) {
  const struct {
    const char *destination;
    const char *source;
    size_t count;
  } inputs[] = {{"", "hello", 0},
                {"", "hello", 5},
                {"hello", " world", 3},
                {"hello", " world", 16}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    char actual[64] = {0};
    char expected[64] = {0};
    strcpy(actual, inputs[i].destination);
    strcpy(expected, inputs[i].destination);

    char *actual_result =
        zenos_strncat(actual, inputs[i].source, inputs[i].count);
    char *expected_result =
        strncat(expected, inputs[i].source, inputs[i].count);

    cr_expect_eq(pointer_offset(actual, sizeof(actual), actual_result),
                 pointer_offset(expected, sizeof(expected), expected_result));
    cr_expect_eq(memcmp(actual, expected, sizeof(actual)), 0,
                 "strncat output differed for case %zu", i);
  }
}

Test(string, strchr_matches_host) {
  const char *inputs[] = {"", "hello", "banana"};
  const int characters[] = {'x', 'h', 'l', 'a', '\0'};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    for (size_t j = 0; j < ARRAY_LEN(characters); j++) {
      cr_expect_eq(pointer_offset(inputs[i], strlen(inputs[i]) + 1,
                                  zenos_strchr(inputs[i], characters[j])),
                   pointer_offset(inputs[i], strlen(inputs[i]) + 1,
                                  strchr(inputs[i], characters[j])),
                   "strchr differed for input %zu, character %d", i,
                   characters[j]);
    }
  }
}

Test(string, strrchr_matches_host) {
  const char *inputs[] = {"", "hello", "banana"};
  const int characters[] = {'x', 'h', 'l', 'a', '\0'};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    for (size_t j = 0; j < ARRAY_LEN(characters); j++) {
      cr_expect_eq(pointer_offset(inputs[i], strlen(inputs[i]) + 1,
                                  zenos_strrchr(inputs[i], characters[j])),
                   pointer_offset(inputs[i], strlen(inputs[i]) + 1,
                                  strrchr(inputs[i], characters[j])),
                   "strrchr differed for input %zu, character %d", i,
                   characters[j]);
    }
  }
}

Test(string, strstr_matches_host) {
  const struct {
    const char *haystack;
    const char *needle;
  } inputs[] = {{"", ""},           {"hello", ""},
                {"hello", "hello"}, {"hello world", "world"},
                {"banana", "ana"},  {"hello", "xyz"}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    cr_expect_eq(
        pointer_offset(inputs[i].haystack, strlen(inputs[i].haystack) + 1,
                       zenos_strstr(inputs[i].haystack, inputs[i].needle)),
        pointer_offset(inputs[i].haystack, strlen(inputs[i].haystack) + 1,
                       strstr(inputs[i].haystack, inputs[i].needle)),
        "strstr differed for case %zu", i);
  }
}

Test(string, strtok_matches_host) {
  const struct {
    const char *input;
    const char *delimiters;
  } inputs[] = {{"", ","},
                {",one,,two,three,", ","},
                {"one two\tthree", " \t"},
                {"no-delimiters", ",;"}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    char actual[64] = {0};
    char expected[64] = {0};
    strcpy(actual, inputs[i].input);
    strcpy(expected, inputs[i].input);

    char *actual_token = zenos_strtok(actual, inputs[i].delimiters);
    char *expected_token = strtok(expected, inputs[i].delimiters);
    size_t token_index = 0;

    while (actual_token != NULL || expected_token != NULL) {
      cr_expect((actual_token == NULL) == (expected_token == NULL),
                "strtok token count differed for case %zu", i);
      if (actual_token == NULL || expected_token == NULL) {
        break;
      }
      cr_expect_str_eq(actual_token, expected_token,
                       "strtok token %zu differed for case %zu", token_index,
                       i);
      cr_expect_eq(pointer_offset(actual, sizeof(actual), actual_token),
                   pointer_offset(expected, sizeof(expected), expected_token),
                   "strtok token %zu location differed for case %zu",
                   token_index, i);
      actual_token = zenos_strtok(NULL, inputs[i].delimiters);
      expected_token = strtok(NULL, inputs[i].delimiters);
      token_index++;
    }

    cr_expect_eq(memcmp(actual, expected, strlen(inputs[i].input) + 1), 0,
                 "strtok buffer differed for case %zu", i);
  }
}

Test(string, memset_matches_host) {
  const struct {
    int value;
    size_t count;
  } inputs[] = {{0, 0}, {0, 8}, {'A', 16}, {0x1ff, 7}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    unsigned char actual[32];
    unsigned char expected[32];
    memset(actual, 0xa5, sizeof(actual));
    memset(expected, 0xa5, sizeof(expected));

    void *actual_result =
        zenos_memset(actual, inputs[i].value, inputs[i].count);
    void *expected_result = memset(expected, inputs[i].value, inputs[i].count);

    cr_expect_eq(pointer_offset(actual, sizeof(actual), actual_result),
                 pointer_offset(expected, sizeof(expected), expected_result));
    cr_expect_eq(memcmp(actual, expected, sizeof(actual)), 0,
                 "memset output differed for case %zu", i);
  }
}

Test(string, memcmp_matches_host) {
  const struct {
    const unsigned char *left;
    const unsigned char *right;
    size_t count;
  } inputs[] = {
      {(const unsigned char *)"abc", (const unsigned char *)"xyz", 0},
      {(const unsigned char *)"hello", (const unsigned char *)"hello", 5},
      {(const unsigned char *)"abc", (const unsigned char *)"abd", 3},
      {(const unsigned char *)"abd", (const unsigned char *)"abc", 3},
      {(const unsigned char *)"hello", (const unsigned char *)"helps", 3}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    int actual = zenos_memcmp(inputs[i].left, inputs[i].right, inputs[i].count);
    int expected = memcmp(inputs[i].left, inputs[i].right, inputs[i].count);
    cr_expect_eq(comparison_sign(actual), comparison_sign(expected),
                 "memcmp differed for case %zu", i);
  }
}

Test(string, memcpy_matches_host) {
  const size_t counts[] = {0, 1, 6, 16, 32};
  const unsigned char source[32] = "a source buffer with some data";

  for (size_t i = 0; i < ARRAY_LEN(counts); i++) {
    unsigned char actual[32];
    unsigned char expected[32];
    memset(actual, 0xa5, sizeof(actual));
    memset(expected, 0xa5, sizeof(expected));

    void *actual_result = zenos_memcpy(actual, source, counts[i]);
    void *expected_result = memcpy(expected, source, counts[i]);

    cr_expect_eq(pointer_offset(actual, sizeof(actual), actual_result),
                 pointer_offset(expected, sizeof(expected), expected_result));
    cr_expect_eq(memcmp(actual, expected, sizeof(actual)), 0,
                 "memcpy output differed for count %zu", counts[i]);
  }
}

Test(string, memmove_matches_host) {
  const struct {
    size_t destination;
    size_t source;
    size_t count;
  } inputs[] = {{0, 8, 0}, {0, 8, 8}, {2, 0, 12}, {0, 6, 12}, {4, 4, 16}};

  for (size_t i = 0; i < ARRAY_LEN(inputs); i++) {
    unsigned char actual[32] = "0123456789abcdefghijklmnopqrstu";
    unsigned char expected[32] = "0123456789abcdefghijklmnopqrstu";

    void *actual_result =
        zenos_memmove(actual + inputs[i].destination, actual + inputs[i].source,
                      inputs[i].count);
    void *expected_result =
        memmove(expected + inputs[i].destination, expected + inputs[i].source,
                inputs[i].count);

    cr_expect_eq(pointer_offset(actual, sizeof(actual), actual_result),
                 pointer_offset(expected, sizeof(expected), expected_result));
    cr_expect_eq(memcmp(actual, expected, sizeof(actual)), 0,
                 "memmove output differed for case %zu", i);
  }
}
