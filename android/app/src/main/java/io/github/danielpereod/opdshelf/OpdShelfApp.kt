package io.github.danielpereod.opdshelf

import android.app.Application
import io.github.danielpereod.opdshelf.data.Api
import io.github.danielpereod.opdshelf.data.LocalStore
import io.github.danielpereod.opdshelf.data.Settings
import io.github.danielpereod.opdshelf.sync.ReadingSync
import org.readium.adapter.pdfium.document.PdfiumDocumentFactory
import org.readium.r2.shared.util.asset.AssetRetriever
import org.readium.r2.shared.util.http.DefaultHttpClient
import org.readium.r2.streamer.PublicationOpener
import org.readium.r2.streamer.parser.DefaultPublicationParser

class OpdShelfApp : Application() {
    lateinit var settings: Settings
    lateinit var api: Api
    lateinit var store: LocalStore
    lateinit var sync: ReadingSync
    lateinit var assetRetriever: AssetRetriever
    lateinit var publicationOpener: PublicationOpener

    override fun onCreate() {
        super.onCreate()
        settings = Settings(this)
        io.github.danielpereod.opdshelf.ui.AppearanceState.mode = settings.appearance
        api = Api(settings)
        store = LocalStore(filesDir)
        sync = ReadingSync(this, settings, api, store)
        val httpClient = DefaultHttpClient()
        assetRetriever = AssetRetriever(contentResolver, httpClient)
        publicationOpener = PublicationOpener(
            DefaultPublicationParser(this, httpClient, assetRetriever, PdfiumDocumentFactory(this))
        )
        sync.schedule()
    }
}

val android.content.Context.app: OpdShelfApp get() = applicationContext as OpdShelfApp
