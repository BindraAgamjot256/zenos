/**
 * ansi_colors.c - ANSI escape code stress test (enhanced)
 */

#include <stdio.h>

/* ANSI escape codes */
#define ESC "\x1b"
#define CSI ESC "["

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
#define BG_WHITE CSI "47m"

/* Bright foreground colors */
#define FG_BRIGHT_RED CSI "91m"
#define FG_BRIGHT_GREEN CSI "92m"
#define FG_BRIGHT_YELLOW CSI "93m"
#define FG_BRIGHT_BLUE CSI "94m"
#define FG_BRIGHT_MAGENTA CSI "95m"
#define FG_BRIGHT_CYAN CSI "96m"
#define FG_BRIGHT_WHITE CSI "97m"

#define CURSOR_HOME CSI "1;1H"
#define CLEAR_SCREEN CSI "2J"

void test_standard_colors(void) {
    printf("\n=== Standard Foreground Colors ===\n");
    printf(FG_BLACK " Black " RESET);
    printf(FG_RED " Red " RESET);
    printf(FG_GREEN " Green " RESET);
    printf(FG_YELLOW " Yellow " RESET);
    printf(FG_BLUE " Blue " RESET);
    printf(FG_MAGENTA " Magenta " RESET);
    printf(FG_CYAN " Cyan " RESET);
    printf(FG_WHITE " White " RESET "\n");
}

void test_text_attributes(void) {
    printf("\n=== Text Attributes ===\n");
    printf(BOLD "Bold " RESET);
    printf(ITALIC "Italic " RESET);
    printf(UNDERLINE "Underline " RESET);
    printf(STRIKETHROUGH "Strike " RESET "\n");
}

void test_256_colors(void) {
    printf("\n=== 256 Color Mode (sample) ===\n");

    for (int i = 0; i < 16; i++)
        printf(CSI "38;5;%dm %3d " RESET, i, i);
    printf("\n");

    for (int i = 16; i < 52; i++) {
        printf(CSI "38;5;%dm %3d " RESET, i, i);
        if ((i - 15) % 6 == 0) printf("\n");
    }

    printf("Grayscale: ");
    for (int i = 232; i < 256; i++)
        printf(CSI "48;5;%dm  " RESET, i);
    printf("\n");

    /* 🔥 FULL 256-COLOR GRADIENT GRID */
    printf("\n=== Full 256-Color Gradient ===\n");
    for (int row = 0; row < 16; row++) {
        for (int col = 0; col < 16; col++) {
            int color = row * 16 + col;
            printf(CSI "48;5;%dm  " RESET, color);
        }
        printf("\n");
    }
}

void test_color_combinations(void) {
    printf("\n=== Color Combinations ===\n");
    printf(FG_RED "R" FG_YELLOW "A" FG_GREEN "I"
           FG_CYAN "N" FG_BLUE "B" FG_MAGENTA "O"
           FG_BRIGHT_RED "W" RESET "\n");

    for (int r = 0; r < 4; r++) {
        for (int c = 0; c < 16; c++)
            printf((r + c) % 2 ? BG_BLACK "  " : BG_WHITE "  ");
        printf(RESET "\n");
    }
}

void test_status_messages(void) {
    printf("\n=== Status Message Styles ===\n");
    printf(FG_BRIGHT_GREEN "[ OK ]" RESET " System initialized\n");
    printf(FG_BRIGHT_YELLOW "[WARN]" RESET " Low memory\n");
    printf(FG_BRIGHT_RED "[FAIL]" RESET " Critical error\n");
    printf(FG_BRIGHT_CYAN "[INFO]" RESET " Processing\n");
}

int main(void) {
    printf(CLEAR_SCREEN CURSOR_HOME);
    printf("ANSI Escape Code Stress Test\n");

    test_standard_colors();
    test_text_attributes();
    test_256_colors();
    test_color_combinations();
    test_status_messages();

    printf("\n" FG_BRIGHT_GREEN "=== All ANSI tests completed ===" RESET "\n");
    return 0;
}
