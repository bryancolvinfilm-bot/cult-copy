# Finish the Mac installer

The macOS implementation is included. A user has successfully built and installed
the Mac package and confirmed Mac-to-PC clipboard sharing works. Native Mac
builds are not available from the Windows development workspace itself. The build
helper below produces the installer on your Mac; the source bundle is not itself
an installed app.

1. Extract `CultCopy-<version>-mac-build-source.zip` on a Mac running macOS 12 or
   newer. Keep the extracted folder somewhere writable.
2. Open `scripts/Build Mac App.command`. Alternatively, open Terminal, type `bash `,
   drag that file into Terminal, and press Return.
3. If Apple Command Line Tools are missing, the helper requests Apple's installer.
   Complete that installation and run the helper again.
4. The helper installs the official minimal Rust toolchain into your user account
   if needed, runs the automated tests, and builds both Apple Silicon and Intel
   versions. An internet connection and several GB of free space are needed.
5. When it succeeds, Finder selects
   `dist/CultCopy-<version>-macos-universal.dmg`. Open it and drag the app to
   **Applications**. A `.pkg` installer is also generated. These are development builds;
   macOS may require your approval to open it.
6. Install the Windows setup on the PC. Open **Devices** on both computers, compare
   the pairing code, and approve on both. Copy a small piece of text on each
   computer and paste on the other. Then try an image and a regular file.

The helper does not install Cult Copy automatically and does not write to
your clipboard during its automated tests. Tests that modify the real clipboard
remain skipped. Build output is saved in `dist/mac-build.log`.

For prebuilt public installers, use the repository's Releases page. To create a
new release through native GitHub runners, follow [publish.md](../publish.md).
Its Mac job creates the universal `.dmg` and `.pkg` without needing a local Mac build.
