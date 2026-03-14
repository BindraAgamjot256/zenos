/**
 * mini_ls.c
 * Simple ls clone supporting:
 *   ls
 *   ls -l
 *   ls -a
 *   ls -la
 */

#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>
#include <errno.h>
#include <dirent.h>
#include <sys/stat.h>

#define PATH_BUF 512

typedef struct {
    int show_all;
    int long_format;
} Options;

static void format_permissions(mode_t mode, char *buf)
{
    buf[0] = S_ISDIR(mode) ? 'd' :
             S_ISLNK(mode) ? 'l' : '-';

    buf[1] = (mode & S_IRUSR) ? 'r' : '-';
    buf[2] = (mode & S_IWUSR) ? 'w' : '-';
    buf[3] = (mode & S_IXUSR) ? 'x' : '-';
    buf[4] = (mode & S_IRGRP) ? 'r' : '-';
    buf[5] = (mode & S_IWGRP) ? 'w' : '-';
    buf[6] = (mode & S_IXGRP) ? 'x' : '-';
    buf[7] = (mode & S_IROTH) ? 'r' : '-';
    buf[8] = (mode & S_IWOTH) ? 'w' : '-';
    buf[9] = (mode & S_IXOTH) ? 'x' : '-';
    buf[10] = '\0';
}

static void print_long(const char *path, const char *name)
{
    char full[PATH_BUF];
    struct stat st;

    snprintf(full, sizeof(full), "%s/%s", path, name);

    if (stat(full, &st) != 0) {
        printf("ls: cannot stat '%s': %d\n", name, errno);
        return;
    }

    char perms[11];
    format_permissions(st.st_mode, perms);

    printf("%s %ld %d %d %ld %s\n",
           perms,
           (long)st.st_nlink,
           st.st_uid,
           st.st_gid,
           (long)st.st_size,
           name);
}

static void print_short(const char *name)
{
    printf("%s\n", name);
}

static int list_dir(const char *path, Options *opt, int header)
{
    DIR *dir = opendir(path);

    if (!dir) {
        printf("ls: cannot access '%s': %d\n", path, errno);
        return 1;
    }

    if (header)
        printf("%s:\n", path);

    struct dirent *entry;

    while ((entry = readdir(dir)) != NULL) {

        if (!opt->show_all && entry->d_name[0] == '.')
            continue;

        if (opt->long_format)
            print_long(path, entry->d_name);
        else
            print_short(entry->d_name);
    }

    closedir(dir);
    return 0;
}

static void parse_flags(int argc, char **argv, Options *opt, int *first_path)
{
    opt->show_all = 0;
    opt->long_format = 0;

    int i;

    for (i = 1; i < argc; i++) {

        if (argv[i][0] != '-')
            break;

        for (size_t j = 1; j < strlen(argv[i]); j++) {

            if (argv[i][j] == 'a')
                opt->show_all = 1;

            else if (argv[i][j] == 'l')
                opt->long_format = 1;

            else {
                printf("ls: unknown option -%c\n", argv[i][j]);
                exit(1);
            }
        }
    }

    *first_path = i;
}

int main(int argc, char *argv[])
{
    Options opt;
    int first_path;

    parse_flags(argc, argv, &opt, &first_path);

    int exit_status = 0;

    if (first_path >= argc) {
        exit_status = list_dir(".", &opt, 0);
    }
    else {

        int multi = (argc - first_path) > 1;

        for (int i = first_path; i < argc; i++) {

            if (i > first_path)
                printf("\n");

            if (list_dir(argv[i], &opt, multi))
                exit_status = 1;
        }
    }

    return exit_status;
}