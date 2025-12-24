// LD_PRELOAD shim that intercepts bind() and replaces any specific IP
// with INADDR_ANY (0.0.0.0)
#include <dlfcn.h>
#include <sys/socket.h>
#include <netinet/in.h>
#include <string.h>
#include <stdio.h>

typedef int (*orig_bind_t)(int, const struct sockaddr *, socklen_t);

int bind(int sockfd, const struct sockaddr *addr, socklen_t addrlen) {
    orig_bind_t orig = (orig_bind_t)dlsym(RTLD_NEXT, "bind");

    if (addr && addr->sa_family == AF_INET) {
        struct sockaddr_in modified;
        memcpy(&modified, addr, sizeof(modified));
        // rewrite to 0.0.0.0 (INADDR_ANY)
        modified.sin_addr.s_addr = htonl(INADDR_ANY);
        return orig(sockfd, (struct sockaddr *)&modified, sizeof(modified));
    }

    return orig(sockfd, addr, addrlen);
}
