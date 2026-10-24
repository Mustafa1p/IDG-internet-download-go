# Windows publisher verification and release signing

The existing v1.4.2 and v1.4.3 downloads are unsigned. A GitHub upload, SHA-256 checksum,
installer metadata, or self-signed certificate cannot make Windows display a
trusted publisher. This repository now supports signing; that does not change
already-published downloads or mean a trusted signing account exists.

The screenshot “Windows protected your PC” is Microsoft Defender SmartScreen's
reputation warning. Windows 11 Smart App Control is a separate execution control
and may also block unknown unsigned executables and DLLs. Signing establishes a
publisher and protects file integrity. SmartScreen can still warn on a new signed
release while reputation builds; even EV certificates do not guarantee a bypass.
Do not tell users to disable Windows security or install a project root certificate.

## Release modes

GitHub tag releases publish unsigned Windows packages by default, without Azure
credentials. Release notes explicitly label these packages as unsigned.

To enable signing, set the **windows-release** environment variable
`IDG_SIGN_RELEASE` to `true` and configure all six variables below. Signing then
becomes required: missing credentials, signing failures, and invalid signatures
stop publication. Leaving `IDG_SIGN_RELEASE` unset or `false` skips Azure login,
signing-client setup, application signing, and installer signature verification.

## One-time owner setup

1. Obtain an identity-validated, **Public Trust** Azure Artifact Signing account
   and certificate profile. Check Microsoft's current geographic/identity
   eligibility before purchasing; a Private Trust or test profile is unsuitable
   for public downloads. If unavailable in your country, use another trusted CA
   with hardware or cloud signing. The GitHub workflow below uses Azure; a different
   provider requires its authentication integration in the release job.
2. Create an Azure application/service principal with a GitHub federated credential:
   issuer `https://token.actions.githubusercontent.com`, audience
   `api://AzureADTokenExchange`, subject
   `repo:Mustafa1p/IDG-internet-download-go:environment:windows-release`.
   Grant **Artifact Signing Certificate Profile Signer** (formerly Trusted Signing
   Certificate Profile Signer) on the chosen certificate profile. No client secret
   or private key belongs in the repository.
3. Create the GitHub environment **windows-release**. Restrict its deployment
   branches/tags to release tags (`v*`) and restrict who can create those tags.
   Set these environment **variables**:

   | Variable | Value |
   | --- | --- |
   | `AZURE_CLIENT_ID` | Federated application's client ID |
   | `AZURE_TENANT_ID` | Azure directory ID |
   | `AZURE_SUBSCRIPTION_ID` | Signing account's subscription ID |
   | `IDG_SIGNING_ENDPOINT` | Account's regional endpoint, e.g. `https://eus.codesigning.azure.net` |
   | `IDG_SIGNING_ACCOUNT` | Artifact Signing account name |
   | `IDG_SIGNING_PROFILE` | Public Trust certificate profile name |

4. Publish a **new version**, updating Cargo.toml/Cargo.lock, the extension manifest,
   installer version, release notes, and README links together, then push its
   matching `vX.Y.Z` tag. Do not replace existing release assets silently. When
   `IDG_SIGN_RELEASE=true`, tag releases fail if signing configuration is missing;
   unsigned releases, main-branch builds, and PR builds need no signing credentials.

With signing enabled, the release job signs the app, native bridge, bundled EXE/DLL media tools, and
portable registration script before packaging. It preserves valid upstream
signatures, rejects broken signatures, and requires a trusted timestamp for every
executable file. Bundled files without upstream signatures receive the release
publisher's signature; review those dependencies before authorizing the signer.
Inno Setup invokes the same signer for Setup, its temporary executable copies,
and the uninstaller. The final installer is verified again. ZIPs and SHA256SUMS.txt
are generated from the signed payload; checksums supplement publisher verification.

## Local signing

Install the Windows SDK x64 SignTool. For an existing certificate/provider exposed
in the current user's certificate store, set `IDG_SIGNING_THUMBPRINT` and
`IDG_TIMESTAMP_URL` (the provider's RFC3161 service), then run:

```powershell
./installer/build-installer.ps1 -SignRelease
```

For Azure, authenticate with `az login`, install .NET 8 and the official x64
Artifact Signing client, and set `IDG_SIGNING_DLIB` to Azure.CodeSigning.Dlib.dll
and `IDG_SIGNING_METADATA` to your account/profile metadata JSON. The same build
command works. `IDG_SIGNTOOL` may override the SDK executable location. Without
`-SignRelease`, the local build is explicitly an unsigned development installer.

Verify a downloaded release in PowerShell:

```powershell
Get-AuthenticodeSignature ./IDG-Setup-vX.Y.Z.exe |
    Format-List Status, SignerCertificate, TimeStamperCertificate
```

Expect `Valid`, the publisher identity validated by your signing provider, and a
timestamp. Check installed IDG.exe, IDG-native.exe, tools EXE/DLLs, and unins000.exe
as well. Test a real browser download on a clean Windows 11 machine with Smart App
Control enabled. CI signature verification cannot promise a particular reputation
decision on every user's device. Microsoft Store distribution is another route
with different SmartScreen treatment, and requires a separate Store submission.

## Primary references

- [Microsoft: SmartScreen reputation](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/smartscreen-reputation)
- [Microsoft: Smart App Control](https://learn.microsoft.com/en-us/windows/apps/develop/smart-app-control/overview)
- [Microsoft: code signing options and eligibility](https://learn.microsoft.com/en-us/windows/apps/package-and-deploy/code-signing-options)
- [Microsoft: Artifact Signing integration](https://learn.microsoft.com/en-us/azure/artifact-signing/how-to-signing-integrations)
- [Inno Setup: SignTool](https://jrsoftware.org/ishelp/topic_setup_signtool.htm)
- [Inno Setup: signed uninstaller](https://jrsoftware.org/ishelp/topic_setup_signeduninstaller.htm)
