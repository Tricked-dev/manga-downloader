#ifndef BBF_RS_FFI_H
#define BBF_RS_FFI_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#pragma pack(push, 1)

typedef struct XXH128_hash_t {
    uint64_t low64;
    uint64_t high64;
} XXH128_hash_t;

typedef struct BBFHeader {
    uint8_t magic[4];
    uint16_t version;
    uint16_t header_len;
    uint32_t flags;
    uint8_t alignment;
    uint8_t ream_size;
    uint16_t reserved_extra;
    uint64_t footer_offset;
    uint8_t reserved[40];
} BBFHeader;

typedef struct BBFFooter {
    uint64_t asset_offset;
    uint64_t page_offset;
    uint64_t section_offset;
    uint64_t meta_offset;
    uint64_t expansion_offset;
    uint64_t string_pool_offset;
    uint64_t string_pool_size;
    uint64_t asset_count;
    uint64_t page_count;
    uint64_t section_count;
    uint64_t meta_count;
    uint64_t expansion_count;
    uint32_t flags;
    uint8_t footer_len;
    uint8_t padding[3];
    uint64_t footer_hash;
    uint8_t reserved[144];
} BBFFooter;

typedef struct BBFAsset {
    uint64_t file_offset;
    uint64_t asset_hash[2];
    uint64_t file_size;
    uint32_t flags;
    uint16_t reserved_value;
    uint8_t media_type;
    uint8_t reserved[9];
} BBFAsset;

typedef struct BBFPage {
    uint64_t asset_index;
    uint32_t flags;
    uint8_t reserved[4];
} BBFPage;

typedef struct BBFSection {
    uint64_t section_title_offset;
    uint64_t section_start_index;
    uint64_t section_parent_offset;
    uint8_t reserved[8];
} BBFSection;

typedef struct BBFMeta {
    uint64_t key_offset;
    uint64_t value_offset;
    uint64_t parent_offset;
    uint8_t reserved[8];
} BBFMeta;

typedef struct BBFExpansion {
    uint64_t exp_reserved[10];
    uint32_t flags;
    uint8_t reserved[44];
} BBFExpansion;

#pragma pack(pop)

typedef struct BBFReader BBFReader;

BBFReader *create_bbf_reader(const char *file);
void close_bbf_reader(BBFReader *reader);

BBFHeader *get_bbf_header(BBFReader *reader);
BBFFooter *get_bbf_footer(BBFReader *reader, BBFHeader *header);

const uint8_t *get_bbf_page_table(BBFReader *reader, BBFFooter *footer);
const uint8_t *get_bbf_asset_table(BBFReader *reader, BBFFooter *footer);
const uint8_t *get_bbf_section_table(BBFReader *reader, BBFFooter *footer);
const uint8_t *get_bbf_meta_table(BBFReader *reader, BBFFooter *footer);
const uint8_t *get_bbf_expansion_table(BBFReader *reader, BBFFooter *footer);

const BBFPage *get_bbf_page_entry(
    BBFReader *reader,
    const uint8_t *table,
    int16_t index
);
const BBFAsset *get_bbf_asset_entry(
    BBFReader *reader,
    const uint8_t *table,
    int32_t index
);
const BBFSection *get_bbf_section_entry(
    BBFReader *reader,
    const uint8_t *table,
    int32_t index
);
const BBFMeta *get_bbf_meta_entry(
    BBFReader *reader,
    const uint8_t *table,
    int32_t index
);
const BBFExpansion *get_bbf_expansion_entry(
    BBFReader *reader,
    const uint8_t *table,
    int32_t index
);

const uint8_t *get_bbf_asset_data(BBFReader *reader, uint64_t file_offset);
const char *get_bbf_string(BBFReader *reader, uint64_t string_offset);
int32_t check_bbf_magic(BBFReader *reader, BBFHeader *header);

XXH128_hash_t compute_asset_hash_from_struct(
    BBFReader *reader,
    const BBFAsset *asset
);
XXH128_hash_t compute_asset_hash_from_index(
    BBFReader *reader,
    uint8_t *table,
    int32_t index
);

#endif /* BBF_RS_FFI_H */
