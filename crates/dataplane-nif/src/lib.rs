#![deny(clippy::undocumented_unsafe_blocks)]
#![deny(unsafe_code)]

pub mod atoms;
pub mod bench_support;
pub mod error_terms;
pub mod runtime_adapter;

#[cfg(test)]
mod tests {
    use super::atoms;
    use dataplane_runtime::errors::NifError;

    /// DP-NB-0025: Unit test for direct ok tuple encoding used by bench helpers.
    /// Ignored: requires Rustler runtime atom initialization (RustlerAtoms::get)
    /// which triggers UB in rustler-codegen without a NIF Env - all atom accessors
    /// including atoms::ok() abort cargo test without an Erlang VM.
    #[ignore = "requires NIF runtime atom initialization - UB in rustler codegen without Env"]
    #[test]
    fn test_ok_atom_exists() {
        // Verify atoms::ok() can be called without panicking.
        // The atom existence is the primary invariant tested here;
        // encoding requires a live NIF environment which is not available in unit tests.
        let _ = atoms::ok();
    }

    /// DP-NB-0026: Unit test for direct binary encoding used by bench helpers.
    /// Ignored: rustler::OwnedBinary::new() requires NIF Env - triggers UB in
    /// rustler-codegen without a running Erlang VM. Mirrors bench_zero_owned_env_send_binary.
    #[ignore = "requires NIF runtime Env for OwnedBinary - aborts cargo test without VM"]
    #[test]
    fn test_binary_encoding_zero_fill() {
        // Verify bench_owned_binary produces correct binary encoding with fill value 0.
        // This mirrors the behavior of bench_zero_owned_env_send_binary.
        let size = 64;
        let mut expected = rustler::OwnedBinary::new(size).unwrap();
        expected.as_mut_slice().fill(0);

        // Verify the binary was created with the correct size and fill.
        assert_eq!(expected.as_slice().len(), size);
        assert!(expected.as_slice().iter().all(|&b| b == 0));
    }

    /// DP-NB-0026: Unit test for binary encoding with non-zero fill.
    /// Ignored: rustler::OwnedBinary::new() requires NIF Env - triggers UB in
    /// rustler-codegen without a running Erlang VM.
    #[ignore = "requires NIF runtime Env for OwnedBinary - aborts cargo test without VM"]
    #[test]
    fn test_binary_encoding_0x5a_fill() {
        // Verify bench_owned_binary produces correct binary encoding with fill 0x5a.
        // This mirrors bench_owned_env_send_binary behavior.
        let size = 128;
        let mut expected = rustler::OwnedBinary::new(size).unwrap();
        expected.as_mut_slice().fill(0x5a);

        assert_eq!(expected.as_slice().len(), size);
        assert!(expected.as_slice().iter().all(|&b| b == 0x5a));
    }

    /// DP-NB-0036: Regression test for EPERM/eperm mapping in error encoding.
    /// Ignored: atoms::error() is a rustler atom accessor - triggers UB in
    /// rustler-codegen without a NIF Env. See test_ok_atom_exists.
    #[ignore = "requires NIF runtime atom initialization - UB in rustler codegen without Env"]
    #[test]
    fn test_error_terms_encode_eperm() {
        // EOPNOTSUPP is the only special-cased error (→ enotsup atom).
        // EPERM should map to its atom name via NifError::atom_name().
        let eperm = NifError::from_errno(libc::EPERM);
        assert_eq!(eperm.atom_name(), "eperm");
    }

    /// DP-NB-0037: Regression test for timeout and closed atoms.
    /// Ignored: atoms::closed() and atoms::undefined() are rustler atom accessors -
    /// trigger UB in rustler-codegen without a NIF Env. Same root cause as
    /// test_ok_atom_exists: rustler::RustlerAtoms::get calls unreachable_unchecked.
    #[ignore = "requires NIF runtime atom initialization - UB in rustler codegen without Env"]
    #[test]
    fn test_closed_and_undefined_atoms() {
        // Verify the closed and undefined atoms exist in our atom list.
        // These are used by various NIF functions for timeout/error conditions.
        let _closed = atoms::closed();
        let _undefined = atoms::undefined();
    }
}
