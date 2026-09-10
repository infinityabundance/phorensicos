/* musl_probe.c — the second-implementation observer for the cross-implementation
 * court (phost::porting::cross_impl).
 *
 * Build:  musl-gcc -static -O1 -o musl_probe musl_probe.c
 *
 * Why this file exists, and why it duplicates logic on purpose
 * -----------------------------------------------------------
 * The dialect cage (phost/src/porting/dialect_cage.rs) observes the *host* C
 * library in-process through a narrow FFI shim. That observation is what the seal
 * binds. It is still one implementation, so "the POSIX contract for `strspn`" is
 * really "the POSIX contract as this host implements it".
 *
 * This probe is the *second* implementation: a program compiled against musl and
 * statically linked, so the functions it calls are musl's and not the host's. The
 * cross-implementation court feeds it the same sealed corpus and requires the two
 * implementations to agree.
 *
 * The per-symbol decoding and normalization below deliberately repeat what the Rust
 * cage does. That repetition is the point: an observer that shared the cage's code
 * could only confirm the cage. The duplication is a feature of the instrument, and
 * the court is self-checking — a mistake here shows up as a disagreement.
 *
 * Protocol
 * --------
 *   argv[1] = symbol (toupper | memcmp | memchr | strlen | strrchr | strspn)
 *   argv[1] = "--which" prints the libc identity this binary was compiled against
 *             and exits, so a caller can prove the probe is really musl.
 *
 *   stdin   one case per line:  <case_id> <arg_hex> [arg_hex ...]
 *   stdout  one case per line:  <case_id> <output_hex>
 *
 * The output encoding matches the court's: a 4-byte little-endian index/sign, an
 * 8-byte little-endian length, or a single folded byte.
 *
 * Scope: an observation instrument for API-surface porting. It is not a binary
 * translator and it is not part of the promoted artifact.
 */

#include <ctype.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define MAX_ARGS 8
#define MAX_BUF 1024
#define MAX_ARGSZ 512

static int hexval(int c)
{
    if (c >= '0' && c <= '9') return c - '0';
    if (c >= 'a' && c <= 'f') return c - 'a' + 10;
    if (c >= 'A' && c <= 'F') return c - 'A' + 10;
    return -1;
}

/* Decode hex into `out`; returns the byte count, or -1 on bad input. */
static long unhex(const char *hex, unsigned char *out, size_t cap)
{
    size_t len = strlen(hex);
    size_t i;
    if (len % 2 != 0) return -1;
    if (len / 2 > cap) return -1;
    for (i = 0; i < len / 2; i++) {
        int hi = hexval((unsigned char)hex[2 * i]);
        int lo = hexval((unsigned char)hex[2 * i + 1]);
        if (hi < 0 || lo < 0) return -1;
        out[i] = (unsigned char)((hi << 4) | lo);
    }
    return (long)(len / 2);
}

/* Little-endian fixed-width encoders: the wire format the court compares. */
static void encode_i32(int32_t v, unsigned char *out)
{
    uint32_t u = (uint32_t)v;
    out[0] = (unsigned char)(u & 0xff);
    out[1] = (unsigned char)((u >> 8) & 0xff);
    out[2] = (unsigned char)((u >> 16) & 0xff);
    out[3] = (unsigned char)((u >> 24) & 0xff);
}

static void encode_u64(uint64_t v, unsigned char *out)
{
    int i;
    for (i = 0; i < 8; i++) out[i] = (unsigned char)((v >> (8 * i)) & 0xff);
}

static uint64_t decode_u64(const unsigned char *b, size_t n)
{
    uint64_t v = 0;
    size_t i;
    for (i = 0; i < n && i < 8; i++) v |= ((uint64_t)b[i]) << (8 * i);
    return v;
}

static void put_hex(const unsigned char *b, size_t n)
{
    size_t i;
    for (i = 0; i < n; i++) printf("%02x", b[i]);
}

/* Copy `src[..len]` and append a NUL when the window has none, so the C-string
 * functions can never read out of bounds. Every corpus case is in contract (the
 * terminator is inside the bound), so the append is defensive. */
static size_t as_c_string(const unsigned char *src, size_t len, unsigned char *dst)
{
    size_t i;
    memcpy(dst, src, len);
    for (i = 0; i < len; i++) {
        if (dst[i] == 0x00) return len;
    }
    dst[len] = 0x00;
    return len + 1;
}

static int sign_of(int v)
{
    if (v < 0) return -1;
    if (v > 0) return 1;
    return 0;
}

static void which(void)
{
#ifdef __GLIBC__
    printf("libc=glibc\n");
#else
    printf("libc=musl\n");
#endif
    printf("pointer_bytes=%zu\n", sizeof(void *));
    printf("int_bytes=%zu\n", sizeof(int));
}

int main(int argc, char **argv)
{
    char line[MAX_BUF];
    const char *symbol;

    if (argc < 2) {
        fprintf(stderr, "usage: musl_probe <symbol> | --which\n");
        return 2;
    }
    symbol = argv[1];
    if (strcmp(symbol, "--which") == 0) {
        which();
        return 0;
    }

    while (fgets(line, sizeof(line), stdin) != NULL) {
        char *tok[MAX_ARGS + 2];
        int ntok = 0;
        char *p = line;
        unsigned char args[MAX_ARGS][MAX_ARGSZ];
        long arglen[MAX_ARGS];
        unsigned char out[8];
        size_t outlen = 0;
        int i;

        memset(arglen, 0, sizeof(arglen));
        memset(args, 0, sizeof(args));

        /* Tokenize on spaces/tabs/newlines. */
        while (*p != '\0' && ntok < MAX_ARGS + 2) {
            while (*p == ' ' || *p == '\t' || *p == '\n' || *p == '\r') p++;
            if (*p == '\0') break;
            tok[ntok++] = p;
            while (*p != '\0' && *p != ' ' && *p != '\t' && *p != '\n' && *p != '\r') p++;
            if (*p != '\0') *p++ = '\0';
        }
        if (ntok < 2) continue; /* blank or malformed line */

        for (i = 1; i < ntok && i <= MAX_ARGS; i++) {
            long n;
            /* `-` is an explicitly empty argument: a bare double space would be
             * indistinguishable from a missing field once tokenized, and the corpus
             * does contain empty arguments (an empty accept set, an empty string). */
            if (strcmp(tok[i], "-") == 0) {
                arglen[i - 1] = 0;
                continue;
            }
            n = unhex(tok[i], args[i - 1], MAX_ARGSZ);
            if (n < 0) {
                fprintf(stderr, "bad hex in case %s\n", tok[0]);
                return 3;
            }
            arglen[i - 1] = n;
        }

        if (strcmp(symbol, "toupper") == 0) {
            if (arglen[0] < 1) return 3;
            out[0] = (unsigned char)toupper((int)args[0][0]);
            outlen = 1;
        } else if (strcmp(symbol, "memcmp") == 0) {
            uint64_t n;
            int rc;
            if (ntok < 4) return 3;
            n = decode_u64(args[2], (size_t)arglen[2]);
            if (n > (uint64_t)arglen[0] || n > (uint64_t)arglen[1]) return 3;
            rc = memcmp(args[0], args[1], (size_t)n);
            encode_i32((int32_t)sign_of(rc), out);
            outlen = 4;
        } else if (strcmp(symbol, "memchr") == 0) {
            uint64_t n;
            unsigned char *found;
            int32_t index = -1;
            if (ntok < 4 || arglen[1] < 1) return 3;
            n = decode_u64(args[2], (size_t)arglen[2]);
            if (n > (uint64_t)arglen[0]) return 3;
            found = (unsigned char *)memchr(args[0], (int)args[1][0], (size_t)n);
            if (found != NULL) index = (int32_t)(found - args[0]);
            encode_i32(index, out);
            outlen = 4;
        } else if (strcmp(symbol, "strlen") == 0) {
            unsigned char buf[MAX_ARGSZ + 1];
            as_c_string(args[0], (size_t)arglen[0], buf);
            encode_u64((uint64_t)strlen((const char *)buf), out);
            outlen = 8;
        } else if (strcmp(symbol, "strrchr") == 0) {
            unsigned char buf[MAX_ARGSZ + 1];
            unsigned char *found;
            int32_t index = -1;
            if (arglen[1] < 1) return 3;
            as_c_string(args[0], (size_t)arglen[0], buf);
            found = (unsigned char *)strrchr((const char *)buf, (int)args[1][0]);
            if (found != NULL) index = (int32_t)(found - buf);
            encode_i32(index, out);
            outlen = 4;
        } else if (strcmp(symbol, "strspn") == 0) {
            unsigned char s[MAX_ARGSZ + 1];
            unsigned char accept[MAX_ARGSZ + 1];
            as_c_string(args[0], (size_t)arglen[0], s);
            as_c_string(args[1], (size_t)arglen[1], accept);
            encode_u64((uint64_t)strspn((const char *)s, (const char *)accept), out);
            outlen = 8;
        } else {
            fprintf(stderr, "unknown symbol %s\n", symbol);
            return 2;
        }

        printf("%s ", tok[0]);
        put_hex(out, outlen);
        printf("\n");
    }

    return 0;
}
