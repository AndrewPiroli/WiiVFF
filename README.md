List or dump the contents of Wii VFF files used by the messageboard.

Currently does not support .mbx WC24 files (wc24recv.mbx or wc24send.mbx)

# Usage

## List

List the contents of the VFF

Usage: wiivff list [OPTIONS] <SRC>

Arguments:
  <SRC>  The path to the input file (cdb.vff)

Options:
      --show-deleted  Show deleted
  -h, --help          Print help

## Dump

Dump the VFF to disk

Usage: wiivff dump [OPTIONS] <SRC> <DEST>

Arguments:
  <SRC>   The path to the input file (cdb.vff)
  <DEST>  Path to dump to

Options:
      --show-deleted  Show deleted
  -h, --help          Print help
