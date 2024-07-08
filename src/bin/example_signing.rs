use frost_secp256k1 as frost;
use rand::thread_rng;

pub fn main() {
    let rng = thread_rng();
    let max_signers = 5;
    let min_signers = 3;
    let (shares, _pubkey_package) = frost::keys::generate_with_dealer(
        max_signers,
        min_signers,
        frost::keys::IdentifierList::Default,
        rng,
    )
    .unwrap();

    for (id, share) in shares {
        println!("ID: {:?} SHARE: {:?}", id, share);
    }
}
