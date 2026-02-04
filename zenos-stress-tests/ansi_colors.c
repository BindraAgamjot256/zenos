/**
 * ansi_colors.c - ANSI escape code stress test
 *
 * TESTS: TTY ANSI escape code parsing, color rendering, cursor movement
 * 
 * EXPECTED BEHAVIOR:
 * - All standard colors (30-37, 40-47) should render correctly
 * - Bright colors (90-97, 100-107) should render correctly
 * - Text attributes (bold, italic, underline, strikethrough) should work
 * - Cursor movement commands should position text correctly
 * - Clear screen and clear line should work
 * - 256-color mode (38;5;n) should work
 *
 * KERNEL BUGS EXPOSED:
 * - ANSI parser state machine bugs
 * - Color palette issues
 * - Cursor positioning off-by-one errors
 * - Buffer overflows in escape sequence parsing
 */

#include <stdio.h>
#include <unistd.h>

/* ANSI escape codes */
#define ESC "\x1b"
#define CSI ESC "["

/* Reset */
#define RESET CSI "0m"

/* Text attributes */
#define BOLD CSI "1m"
#define ITALIC CSI "3m"
#define UNDERLINE CSI "4m"
#define STRIKETHROUGH CSI "9m"

/* Standard foreground colors */
#define FG_BLACK CSI "30m"
#define FG_RED CSI "31m"
#define FG_GREEN CSI "32m"
#define FG_YELLOW CSI "33m"
#define FG_BLUE CSI "34m"
#define FG_MAGENTA CSI "35m"
#define FG_CYAN CSI "36m"
#define FG_WHITE CSI "37m"

/* Standard background colors */
#define BG_BLACK CSI "40m"
#define BG_RED CSI "41m"
#define BG_GREEN CSI "42m"
#define BG_YELLOW CSI "43m"
#define BG_BLUE CSI "44m"
#define BG_MAGENTA CSI "45m"
#define BG_CYAN CSI "46m"
#define BG_WHITE CSI "47m"

/* Bright foreground colors */
#define FG_BRIGHT_BLACK CSI "90m"
#define FG_BRIGHT_RED CSI "91m"
#define FG_BRIGHT_GREEN CSI "92m"
#define FG_BRIGHT_YELLOW CSI "93m"
#define FG_BRIGHT_BLUE CSI "94m"
#define FG_BRIGHT_MAGENTA CSI "95m"
#define FG_BRIGHT_CYAN CSI "96m"
#define FG_BRIGHT_WHITE CSI "97m"

/* Cursor movement */
#define CURSOR_HOME CSI "H"
#define CLEAR_SCREEN CSI "2J"
#define CLEAR_LINE CSI "2K"

void test_standard_colors(void) {
    printf("\n=== Standard Foreground Colors ===\n");
    printf(FG_BLACK   "  Black  " RESET);
    printf(FG_RED     "  Red    " RESET);
    printf(FG_GREEN   "  Green  " RESET);
    printf(FG_YELLOW  "  Yellow " RESET);
    printf(FG_BLUE    "  Blue   " RESET);
    printf(FG_MAGENTA "  Magenta" RESET);
    printf(FG_CYAN    "  Cyan   " RESET);
    printf(FG_WHITE   "  White  " RESET);
    printf("\n");
}

void test_bright_colors(void) {
    printf("\n=== Bright Foreground Colors ===\n");
    printf(FG_BRIGHT_BLACK   "  Black  " RESET);
    printf(FG_BRIGHT_RED     "  Red    " RESET);
    printf(FG_BRIGHT_GREEN   "  Green  " RESET);
    printf(FG_BRIGHT_YELLOW  "  Yellow " RESET);
    printf(FG_BRIGHT_BLUE    "  Blue   " RESET);
    printf(FG_BRIGHT_MAGENTA "  Magenta" RESET);
    printf(FG_BRIGHT_CYAN    "  Cyan   " RESET);
    printf(FG_BRIGHT_WHITE   "  White  " RESET);
    printf("\n");
}

void test_background_colors(void) {
    printf("\n=== Background Colors ===\n");
    printf(BG_RED     FG_WHITE "  Red    " RESET " ");
    printf(BG_GREEN   FG_BLACK "  Green  " RESET " ");
    printf(BG_YELLOW  FG_BLACK "  Yellow " RESET " ");
    printf(BG_BLUE    FG_WHITE "  Blue   " RESET " ");
    printf(BG_MAGENTA FG_WHITE "  Magenta" RESET " ");
    printf(BG_CYAN    FG_BLACK "  Cyan   " RESET " ");
    printf("\n");
}

void test_text_attributes(void) {
    printf("\n=== Text Attributes ===\n");
    printf(BOLD "  Bold Text  " RESET);
    printf(ITALIC "  Italic Text  " RESET);
    printf(UNDERLINE "  Underlined  " RESET);
    printf(STRIKETHROUGH "  Strikethrough  " RESET);
    printf("\n");
    
    /* Combined attributes */
    printf(BOLD FG_RED "  Bold Red  " RESET);
    printf(UNDERLINE FG_BLUE "  Underline Blue  " RESET);
    printf(BOLD UNDERLINE FG_GREEN "  Bold+Underline Green  " RESET);
    printf("\n");
}

void test_256_colors(void) {
    printf("\n=== 256 Color Mode (sample) ===\n");
    
    /* Show a subset of the 256 color palette */
    for (int i = 0; i < 16; i++) {
        printf(CSI "38;5;%dm %3d " RESET, i, i);
    }
    printf("\n");
    
    /* Show some of the 6x6x6 color cube */
    for (int i = 16; i < 52; i++) {
        printf(CSI "38;5;%dm %3d " RESET, i, i);
        if ((i - 15) % 6 == 0) printf("\n");
    }
    
    /* Show grayscale ramp */
    printf("Grayscale: ");
    for (int i = 232; i < 256; i++) {
        printf(CSI "48;5;%dm  " RESET, i);
    }
    printf("\n");
}

void test_color_combinations(void) {
    printf("\n=== Color Combinations ===\n");
    
    /* Rainbow text */
    printf(FG_RED "R" FG_YELLOW "A" FG_GREEN "I" FG_CYAN "N" FG_BLUE "B" FG_MAGENTA "O" FG_RED "W" RESET "\n");
    
    /* Checkerboard pattern */
    for (int row = 0; row < 4; row++) {
        for (int col = 0; col < 16; col++) {
            if ((row + col) % 2 == 0) {
                printf(BG_WHITE "  " RESET);
            } else {
                printf(BG_BLACK "  " RESET);
            }
        }
        printf("\n");
    }
}

void test_status_messages(void) {
    printf("\n=== Status Message Styles ===\n");
    printf(FG_GREEN "[  OK  ]" RESET " System initialized\n");
    printf(FG_YELLOW "[ WARN ]" RESET " Low memory condition\n");
    printf(FG_RED "[ FAIL ]" RESET " Critical error occurred\n");
    printf(FG_CYAN "[ INFO ]" RESET " Processing request\n");
    printf(FG_BLUE "[ DEBUG]" RESET " Variable x = 42\n");
}

int main(int argc, char *argv[]) {
    (void)argc; (void)argv;
    
    printf(CLEAR_SCREEN CURSOR_HOME);
    printf("========================================\n");
    printf("   ANSI Escape Code Stress Test\n");
    printf("========================================\n");
    
    test_standard_colors();
    test_bright_colors();
    test_background_colors();
    test_text_attributes();
    test_256_colors();
    test_color_combinations();
    test_status_messages();
    
    printf("\n" FG_GREEN "=== All ANSI tests completed! ===" RESET "\n\n");
    
    exit(0);
    return 0;
}
