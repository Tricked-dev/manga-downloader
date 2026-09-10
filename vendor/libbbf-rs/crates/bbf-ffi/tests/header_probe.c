#include "bbf_ffi.h"

#include <stddef.h>

_Static_assert(sizeof(XXH128_hash_t) == 16, "XXH128_hash_t layout changed");
_Static_assert(sizeof(BBFHeader) == 64, "BBFHeader layout changed");
_Static_assert(sizeof(BBFFooter) == 256, "BBFFooter layout changed");
_Static_assert(sizeof(BBFAsset) == 48, "BBFAsset layout changed");
_Static_assert(sizeof(BBFPage) == 16, "BBFPage layout changed");
_Static_assert(sizeof(BBFSection) == 32, "BBFSection layout changed");
_Static_assert(sizeof(BBFMeta) == 32, "BBFMeta layout changed");
_Static_assert(sizeof(BBFExpansion) == 128, "BBFExpansion layout changed");

int main(void) {
    BBFReader *reader = create_bbf_reader(0);
    BBFHeader *header = get_bbf_header(reader);
    BBFFooter *footer = get_bbf_footer(reader, header);
    const uint8_t *assets = get_bbf_asset_table(reader, footer);
    const BBFAsset *asset = get_bbf_asset_entry(reader, assets, 0);
    (void)compute_asset_hash_from_struct(reader, asset);
    (void)compute_asset_hash_from_index(reader, (uint8_t *)assets, 0);
    close_bbf_reader(reader);
    return 0;
}
