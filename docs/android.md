# Android app

[`android/`](../android) is an app for reading the library on Android, also offline. Download it from the web panel's devices page, or from the [`android-latest`](https://github.com/DanielPereod/opdshelf/releases/tag/android-latest) release.

- **My books**: the books on the phone, with a **Continue reading** card, filters (All, Reading, Not started, Finished, with counts), search, sorting and a ⋮ menu on every book. Long press to select several and mark them finished or remove them.
- **Explore**: browse and search the catalog by recent books, A–Z, authors, series and folders, as a grid or a list, and download books (EPUB, PDF, CBZ). Downloads can be cancelled. Without a connection a band says so and the downloaded books are still at hand.
- **Folders**: in Explore → Folders, the folder button creates a folder where you are, and a book's details (or its ⋮ menu) has **Move to folder**: tap a folder to move it there, or type a new name to create it and move it in.
- **Book details** with cover, series, publisher, year, language and subjects; two panes on tablets.
- **Reader** built on [Readium](https://readium.org): light, sepia and dark themes (the bars follow them), fonts, margins, justification, brightness, minutes left in the chapter, contents, bookmarks, search inside the book, previous/next chapter, volume keys to turn pages and the screen kept on while reading.
- **Sync**: the position is saved on the phone and sent to the server when there is a connection, so it is shared with KOReader and the web reader. When a book was read further on another device, it opens there and says so. Select text and choose **Set as last position** to pin a sentence, as in KOReader. The last page of a book saves 100 %, so it counts as finished everywhere.
- Mark a book as finished by hand, from the menu or its details.
- Works with large fonts (up to 200 %), in landscape and on tablets, and follows the system theme or a chosen one.
- EPUB positions are KOReader xpointers, worked out with the same script as the web reader.

In the app's **Settings**, enter the server address (`192.168.1.10:3000` or `https://books.example.com`), OPDShelf's username and password, and a name for the device.

To build it you need JDK 17 or newer and the Android SDK (platform 37):

```sh
cd android
./gradlew assembleRelease   # app/build/outputs/apk/release/app-release.apk
./gradlew testDebugUnitTest
```

Builds without a release key are signed with a test key kept in the repository (`android/app/opdshelf-test.keystore`), so a newer test APK installs over an older one. CI builds one on every pull request; download it from the run's artifacts. Since the test key is public, those APKs are for testing only. They install as a separate app, "OPDShelf test" (`io.github.danielpereod.opdshelf.test`), next to the real one: Android won't update an app with an APK signed with another key, so sharing the package would block the real app's updates until it was uninstalled.

## Publishing a release

Pushing a tag like `android-v1.2.0` builds the APK, signs it with the private release key and publishes it in GitHub Releases as `opdshelf-1.2.0.apk`. The version code comes from the tag (1.2.0 → 10200), so each release installs over the previous one.

Besides those versioned releases, every push to `main` or `beta` that touches the app publishes a build to the `android-latest` or `android-beta` release, which is what the web panel offers. Its version code comes from the commit date, so each build installs over the previous one, and it is signed with the release key when the repository has one (otherwise with the test key, as the separate test app).

The release key is set up once. Create it and keep a copy of the file and its password somewhere safe: without them, later versions can no longer update the installed app.

```sh
keytool -genkeypair -keystore opdshelf-release.keystore -alias opdshelf \
  -keyalg RSA -keysize 4096 -validity 36500
gh secret set ANDROID_KEYSTORE_BASE64 < <(base64 -w0 opdshelf-release.keystore)
gh secret set ANDROID_KEYSTORE_PASSWORD   # the password chosen above
gh secret set ANDROID_KEY_PASSWORD        # the same password, unless a different one was given for the key
gh secret set ANDROID_KEY_ALIAS --body opdshelf
```

Then:

```sh
git tag android-v1.0.0 && git push origin android-v1.0.0
```

Without those secrets the release workflow stops instead of publishing an APK signed with the public test key.
