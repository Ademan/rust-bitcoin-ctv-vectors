# What?

This tool is intended to generate random BIP-119 test vectors similar to those in [bips/bip-0119/vectors/ctvhash.json](https://github.com/bitcoin/bips/blob/master/bip-0119/vectors/ctvhash.json).
amounts, scriptPubkeys, outpoints, scriptSigs, and witnesses are all randomized while targeting a certain average byte size.
Amounts are constrained to be less than or equal to `MAX_MONEY` to prevent issues deserializing in `rust-bitcoin`.
These vectors should be used to test the calculation of the default template in BIP-119 implementations outside of Bitcoin Core.
This tool uses Bitcoin Core's BIP-119 implementation as a ground truth by exercising a new RPC command `getdefaulttemplate` which is implemented in a fork of Bitcoin Core [here](https://github.com/Ademan/bitcoin/tree/ctv-rpc).

# Why?
The test vectors included in the BIP cannot be used with [rust-bitcoin](https://github.com/rust-bitcoin/rust-bitcoin) because they violate the `MAX_MONEY` constraint. (See discussion in [rust-bitcoin issue #4273](https://github.com/rust-bitcoin/rust-bitcoin/issues/4273))
