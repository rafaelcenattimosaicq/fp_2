// LD_PRELOAD shim that patches epoll_wait() to enforce a minimum 1ms
// timeout. The gRPC C core on ARM64 does an epoll_wait(timeout=0)
// busy-loop that pins a CPU core at 100%. Found this the hard way
// when a Pi4 at the Joinville plant thermal-throttled after 20 min.
//
// Build: gcc -shared -fPIC -o epoll_timeout.so epoll_timeout.c -ldl

#define _GNU_SOURCE
#include <dlfcn.h>
#include <sys/epoll.h>

typedef int (*orig_epoll_wait_t)(int, struct epoll_event *, int, int);

int epoll_wait(int epfd, struct epoll_event *events, int maxevents, int timeout) {
    orig_epoll_wait_t orig = (orig_epoll_wait_t)dlsym(RTLD_NEXT, "epoll_wait");
    if (timeout == 0) timeout = 1; // min 1ms to avoid busy-loop
    return orig(epfd, events, maxevents, timeout);
}
