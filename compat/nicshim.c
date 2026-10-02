// 兼容垫片：把 getifaddrs() 结果中 ifa_addr == NULL 的条目从链表摘除。
//
// 背景（2026-10-02 实机定位）：FlClash（mihomo）的 TUN 接口在 glibc
// getifaddrs() 结果里会出现 ifa_addr == NULL 的条目；官方 2014 客户端在
// get_nics_info 里遍历该链表并直接解引用 ifa_addr->sa_family（没有空指针
// 检查），于是启动约 3 秒后确定性 SIGSEGV。垫片只做一件事：把这类条目从
// 链表中摘掉，其余条目原样保留，对调用方别无副作用。
//
// 内存语义：glibc 的 getifaddrs 结果是一整块分配（freeifaddrs 释放整块），
// 摘链只改 ifa_next 指针，不需要也不能单独释放被摘除的节点，因此不泄漏。
//
// 集成方式：install.sh 将其编译为 libexec 目录下的 nicshim.so，官方客户端
// wrapper 在 exec 前按需 LD_PRELOAD；未加载时不参与任何执行路径。
#define _GNU_SOURCE
#include <dlfcn.h>
#include <ifaddrs.h>
#include <stddef.h>

typedef int (*getifaddrs_fn)(struct ifaddrs **);

int getifaddrs(struct ifaddrs **ifap) {
    static getifaddrs_fn real = NULL;
    if (real == NULL) {
        real = (getifaddrs_fn)dlsym(RTLD_NEXT, "getifaddrs");
        if (real == NULL) {
            return -1;
        }
    }
    int rc = real(ifap);
    if (rc != 0 || ifap == NULL || *ifap == NULL) {
        return rc;
    }
    struct ifaddrs *prev = NULL;
    struct ifaddrs *cur = *ifap;
    while (cur != NULL) {
        struct ifaddrs *next = cur->ifa_next;
        if (cur->ifa_addr == NULL) {
            if (prev != NULL) {
                prev->ifa_next = next;
            } else {
                *ifap = next;
            }
        } else {
            prev = cur;
        }
        cur = next;
    }
    return rc;
}
