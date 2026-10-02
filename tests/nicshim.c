// 垫片行为自测：统计 getifaddrs() 链表的条目总数与 ifa_addr == NULL 的
// 条目数，并在存在 NULL 条目时以退出码 1 失败——因此"带垫片运行必须退出 0"
// 就是本测试的全部断言。
//
// 合成模式：以 -DNICSHIM_TEST_FAKE 编译成共享库时，本文件伪装出"带 NULL
// 条目的 getifaddrs"，配合 LD_PRELOAD="nicshim.so nicshim-fake.so" 为 CI
// 提供不依赖宿主机网卡状况的确定性输入——垫片用 dlsym(RTLD_NEXT) 找到本库，
// 先摘除合成的 NULL 节点，测试再读结果。单独 preload fake 运行必须能看到
// 这两个 NULL 条目，用于证明断言确有区分力。
#include <ifaddrs.h>
#include <stdio.h>

#ifdef NICSHIM_TEST_FAKE
#include <netinet/in.h>
#include <sys/socket.h>

// 三个节点：NULL 在头部、正常节点在中间、NULL 在尾部——覆盖垫片在链表
// 首、中、尾三处摘链的分支。
static struct ifaddrs fake_head;
static struct ifaddrs fake_middle;
static struct ifaddrs fake_tail;
static struct sockaddr_in fake_addr = {.sin_family = AF_INET};

int getifaddrs(struct ifaddrs **ifap) {
    if (ifap == NULL) {
        return -1;
    }
    fake_head.ifa_next = &fake_middle;
    fake_head.ifa_name = "flclash-head";
    fake_head.ifa_addr = NULL;
    fake_middle.ifa_next = &fake_tail;
    fake_middle.ifa_name = "lo";
    fake_middle.ifa_addr = (struct sockaddr *)&fake_addr;
    fake_tail.ifa_next = NULL;
    fake_tail.ifa_name = "flclash-tail";
    fake_tail.ifa_addr = NULL;
    *ifap = &fake_head;
    return 0;
}

// 合成链表是静态存储：freeifaddrs 必须一并伪装成空操作，否则 libc 会去释放
// 并不存在的整块分配。
void freeifaddrs(struct ifaddrs *ifa) {
    (void)ifa;
}
#else
int main(void) {
    struct ifaddrs *list = NULL;
    if (getifaddrs(&list) != 0) {
        perror("getifaddrs");
        return 2;
    }
    int total = 0;
    int nulls = 0;
    for (struct ifaddrs *cur = list; cur != NULL; cur = cur->ifa_next) {
        total++;
        if (cur->ifa_addr == NULL) {
            nulls++;
            printf("NULL: %s\n", cur->ifa_name);
        }
    }
    freeifaddrs(list);
    printf("total=%d null=%d\n", total, nulls);
    return nulls == 0 ? 0 : 1;
}
#endif
