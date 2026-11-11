# Bundled apps

The Docker publish workflow builds the Android app into `apps/opdshelf.apk`
before building the image, so every OPDShelf container can offer it for
download in the web panel (Devices → Connect your apps).

An APK copied to `DATA_DIR/apps/opdshelf.apk` on a server takes the place of
the bundled one.
