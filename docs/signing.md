# Signing

`com.fnpilot.preview` is a separate development identity. It does not promise continuity with the Swift app's `com.fnpilot.mac` permissions. Local unsigned/ad-hoc or Apple Development builds are not official releases.

Official Mac distribution requires Developer ID, hardened runtime as applicable, accepted notarization and a stapled ticket. Windows requires the configured Authenticode signer. Credentials must stay outside the repository and fork CI. Record the actual designated requirement or certificate thumbprint in release evidence and verify same-signer upgrades on native machines.

The standalone Swift app and its signing scripts have been retired. The new release verification scripts reject development/unsigned artifacts when checking an official release. No signing identity is created, bought or embedded by migration code.
