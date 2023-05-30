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

typedef struct String String;

typedef struct Vec_u8 Vec_u8;

typedef struct WalletConfig WalletConfig;

typedef struct WalletData WalletData;

typedef struct Account {
  uint8_t private_key[PRIVATE_KEY_LEN];
  uint8_t public_key[PUBLIC_KEY_LEN];
  uint8_t program_hash[UINT160SIZE];
} Account;

typedef struct Wallet {
  struct WalletConfig config;
  struct Account account;
  struct WalletData wallet_data;
} Wallet;

struct Wallet random_wallet(void);

char *rust_greeting(const char *to);

void rust_greeting_free(char *s);

struct String to_json(const struct Wallet *self);

struct Vec_u8 to_json_char(const struct Wallet *self);
