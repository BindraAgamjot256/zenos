; crt0.asm - C runtime startup for x86_64
; Called by kernel with stack layout:
;   [rsp]     = argc
;   [rsp+8]   = argv[0]
;   [rsp+16]  = argv[1]
;   ...
;   [rsp+8*(argc+1)] = NULL (argv terminator)
;   [rsp+8*(argc+2)] = envp[0]
;   ...
;   NULL (envp terminator)

section .text
global _start
extern main
extern exit
extern __libc_init

_start:
    ; Clear frame pointer for debuggers
    xor rbp, rbp

    ; Get argc from stack
    mov rdi, [rsp]          ; argc -> rdi (1st arg)

    ; Calculate argv = rsp + 8
    lea rsi, [rsp + 8]      ; argv -> rsi (2nd arg)

    ; Calculate envp = argv + (argc + 1) * 8
    mov rax, rdi            ; rax = argc
    add rax, 1              ; rax = argc + 1
    shl rax, 3              ; rax = (argc + 1) * 8
    lea rdx, [rsi + rax]    ; envp -> rdx

    ; Save argc, argv across __libc_init call
    push rdi                ; save argc
    push rsi                ; save argv

    ; Initialize libc with envp
    mov rdi, rdx            ; envp -> rdi (1st arg to __libc_init)
    call __libc_init

    ; Restore argc, argv
    pop rsi                 ; restore argv
    pop rdi                 ; restore argc

    ; Call main(argc, argv) - no envp parameter
    call main

    ; Exit with return value from main
    mov rdi, rax            ; exit code = return value of main
    call exit

    ; Should never reach here, but just in case
.hang:
    hlt
    jmp .hang