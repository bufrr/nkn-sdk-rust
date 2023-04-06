#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>


#define AES_HASH_LEN 32

#define CHECKSUM_LEN 4

#define DEFAULT_RPC_CONCURRENCY 10

#define DEFAULT_RPC_TIMEOUT 10

#define IV_LEN 16

#define MASTER_KEY_LEN 32

#define PRIVATE_KEY_LEN 32

#define PUBLIC_KEY_LEN 32

#define RIPEMD160_LEN 20

#define SCRYPT_KEY_LEN 32

#define SCRYPT_LOG_N 15

#define SCRYPT_P 1

#define SCRYPT_R 8

#define SCRYPT_SALT_LEN 8

#define SEED_LEN 32

#define SHA256_LEN 32

#define SIGNATURE_LEN 64

#define UINT160SIZE 20

#define WALLET_VERSION 2

typedef struct Account Account;

typedef struct Result_Wallet__String Result_Wallet__String;

typedef struct WalletConfig WalletConfig;

struct Result_Wallet__String new(struct Account account, struct WalletConfig config);
