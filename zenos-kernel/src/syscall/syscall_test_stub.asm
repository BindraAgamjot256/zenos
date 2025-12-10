section .text
global _start

[BITS 64]
_start:
    mov rax, 1          ; syscall: sys_write
    mov rdi, 1          ; file descriptor: stdout
    lea rsi, [rel message] ; pointer to the message
    mov rdx, 22         ; message length
    syscall             ; invoke kernel

    mov rax, 60         ; syscall: sys_exit
    xor rdi, rdi        ; exit status 0
    syscall             ; invoke kernel

section .data
message db "hello from user stub!", 0x0a
