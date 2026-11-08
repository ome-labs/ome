use os_rng::SysRng;
use rand_core::UnwrapErr;

/// The operating system's CSPRNG as a `rand_core` RNG (panics only if the OS cannot provide randomness).
pub(crate) fn os_rng() -> UnwrapErr<SysRng> {
    UnwrapErr(SysRng)
}

/// `N` random bytes from the OS.
pub(crate) fn random_bytes<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    rand_core::Rng::fill_bytes(&mut os_rng(), &mut out);
    out
}
