//! The web panel in English or Spanish.
//!
//! Texts are written in English in the templates (`{{t "All books"}}`) and in
//! the scripts (`t('Uploading…')`), and looked up in [`SPANISH`]. The language
//! comes from the `opdshelf_lang` cookie, set from the panel, or else from the
//! browser's Accept-Language.

use axum::http::{header, HeaderMap};
use handlebars::{Context, Handlebars, Helper, HelperDef, RenderContext, RenderError, ScopedJson};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::OnceLock;

pub const COOKIE: &str = "opdshelf_lang";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lang {
    #[default]
    En,
    Es,
}

impl Lang {
    pub fn parse(code: &str) -> Option<Self> {
        let primary = code.trim().split(['-', '_']).next()?.to_ascii_lowercase();
        match primary.as_str() {
            "en" => Some(Self::En),
            "es" => Some(Self::Es),
            _ => None,
        }
    }

    pub fn from_headers(headers: &HeaderMap) -> Self {
        if let Some(lang) = crate::auth::cookie(headers, COOKIE).and_then(|c| Self::parse(&c)) {
            return lang;
        }
        let accept = headers
            .get(header::ACCEPT_LANGUAGE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        // The supported language the browser prefers most
        let mut wanted: Vec<(f32, Self)> = accept
            .split(',')
            .filter_map(|part| {
                let mut pieces = part.split(';');
                let lang = Self::parse(pieces.next()?)?;
                let q = pieces
                    .find_map(|p| p.trim().strip_prefix("q="))
                    .and_then(|q| q.parse().ok())
                    .unwrap_or(1.0);
                Some((q, lang))
            })
            .collect();
        wanted.sort_by(|a, b| b.0.total_cmp(&a.0));
        wanted.first().map_or(Self::En, |w| w.1)
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::En => "en",
            Self::Es => "es",
        }
    }

    /// A text of the panel in this language.
    pub fn text(self, english: &str) -> String {
        match self {
            Self::En => english.to_string(),
            Self::Es => spanish()
                .get(english)
                .map_or_else(|| english.to_string(), |s| s.to_string()),
        }
    }

    /// A message built at run time, such as "Uploaded 3 book(s)", translated
    /// by matching it against the texts that have `{placeholders}`.
    pub fn message(self, message: &str) -> String {
        if self == Self::En {
            return message.to_string();
        }
        if let Some(text) = spanish().get(message) {
            return text.to_string();
        }
        for (english, translated) in SPANISH {
            if !english.contains('{') {
                continue;
            }
            if let Some(values) = match_template(english, message) {
                return fill(translated, &values);
            }
        }
        message.to_string()
    }

    /// The texts the scripts of a page need, as a JSON object.
    pub fn table(self) -> Value {
        match self {
            Self::En => json!({}),
            Self::Es => json!(spanish()),
        }
    }
}

fn spanish() -> &'static HashMap<&'static str, &'static str> {
    static TABLE: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    TABLE.get_or_init(|| SPANISH.iter().copied().collect())
}

/// Replaces `{name}` with its value.
pub fn fill(text: &str, values: &[(String, String)]) -> String {
    let mut out = text.to_string();
    for (name, value) in values {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

/// Matches `message` against a text with `{placeholders}` and returns their values.
fn match_template(template: &str, message: &str) -> Option<Vec<(String, String)>> {
    // Split into literal pieces and placeholder names
    let mut literals = Vec::new();
    let mut names = Vec::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        let close = rest[open..].find('}')? + open;
        literals.push(&rest[..open]);
        names.push(&rest[open + 1..close]);
        rest = &rest[close + 1..];
    }
    literals.push(rest);

    let mut remaining = message.strip_prefix(literals[0])?;
    let mut values = Vec::new();
    for (i, name) in names.iter().enumerate() {
        let next = literals[i + 1];
        let end = if next.is_empty() {
            if i + 1 == names.len() {
                remaining.len()
            } else {
                return None;
            }
        } else if i + 2 == literals.len() {
            // The last literal must end the message
            remaining.strip_suffix(next).map(str::len)?
        } else {
            remaining.find(next)?
        };
        let value = &remaining[..end];
        if value.is_empty() {
            return None;
        }
        values.push((name.to_string(), value.to_string()));
        remaining = &remaining[end + next.len()..];
    }
    remaining.is_empty().then_some(values)
}

/// `{{t "English text" name=value}}`: the text in the page's language.
pub struct TranslateHelper;

impl HelperDef for TranslateHelper {
    fn call_inner<'reg: 'rc, 'rc>(
        &self,
        h: &Helper<'rc>,
        _: &'reg Handlebars<'reg>,
        ctx: &'rc Context,
        _: &mut RenderContext<'reg, 'rc>,
    ) -> Result<ScopedJson<'rc>, RenderError> {
        let english = h.param(0).and_then(|p| p.value().as_str()).unwrap_or_default();
        let lang = ctx
            .data()
            .get("lang")
            .and_then(Value::as_str)
            .and_then(Lang::parse)
            .unwrap_or_default();
        let values: Vec<(String, String)> = h
            .hash()
            .iter()
            .map(|(name, value)| {
                let text = match value.value() {
                    Value::String(s) => s.clone(),
                    Value::Null => String::new(),
                    other => other.to_string(),
                };
                (name.to_string(), text)
            })
            .collect();
        Ok(ScopedJson::Derived(Value::String(fill(&lang.text(english), &values))))
    }
}

/// English text → Spanish.
///
/// Texts with `{placeholders}` also translate messages built at run time, in
/// this order, so a specific one goes before a general one.
pub const SPANISH: &[(&str, &str)] = &[
    ("Shelves", "Estanterías"),
    ("Not found - OPDShelf", "No encontrado - OPDShelf"),
    ("This page does not exist", "Esta página no existe"),
    (
        "The book may have been moved, renamed or deleted, or the link is incomplete.",
        "Puede que el libro se haya movido, renombrado o borrado, o que el enlace esté incompleto.",
    ),
    ("Go to the library", "Ir a la biblioteca"),
    ("Go back", "Volver"),
    ("Sort by", "Ordenar por"),
    ("Select", "Seleccionar"),
    ("YYYY-MM-DD", "AAAA-MM-DD"),
    ("Read to the end", "Terminado"),
    ("Read (done)", "Leído"),
    ("Unread", "No leído"),
    ("A year, or a date as YYYY-MM-DD", "Un año, o una fecha como AAAA-MM-DD"),
    ("A language code such as es or en-GB", "Un código de idioma como es o en-GB"),
    ("Reading progress", "Lectura"),
    ("Fit the whole page", "Ver la página entera"),
    ("Fit the width", "Ajustar al ancho"),
    ("Page {page} of {total}", "Página {page} de {total}"),
    ("Folder…", "Carpeta…"),
    ("Clear search", "Borrar búsqueda"),
    ("Show all ({n})", "Ver todos ({n})"),
    ("Show fewer", "Ver menos"),
    ("Theme: {name}", "Tema: {name}"),
    ("System", "Sistema"),
    ("Light", "Claro"),
    ("Dark", "Oscuro"),
    ("Mark as read", "Marcar como leído"),
    ("Mark as unread", "Marcar como no leído"),
    ("Marked as read", "Marcado como leído"),
    ("Marked as unread", "Marcado como no leído"),
    ("{n} books marked as read", "{n} libros marcados como leídos"),
    ("{n} books marked as unread", "{n} libros marcados como no leídos"),
    ("Text settings", "Ajustes del texto"),
    ("Text size", "Tamaño del texto"),
    ("Justify text", "Justificar el texto"),
    ("Font", "Fuente"),
    ("Serif", "Con serifa"),
    ("Sans", "Sin serifa"),
    ("Line spacing", "Interlineado"),
    ("Tight", "Junto"),
    ("Normal", "Normal"),
    ("Loose", "Amplio"),
    ("Margins", "Márgenes"),
    ("Narrow", "Estrechos"),
    ("Wide", "Anchos"),
    ("Book series", "Series"),
    ("Show the books of {name}", "Ver los libros de {name}"),
    ("Author", "Autor"),
    ("Newest first", "Más recientes"),
    ("Title A–Z", "Título A–Z"),
    ("Title Z–A", "Título Z–A"),
    ("Select books", "Seleccionar libros"),
    ("Skip to the books", "Saltar a los libros"),
    ("{column}, sorted {order}", "{column}, orden {order}"),
    ("descending", "descendente"),
    ("ascending", "ascendente"),
    ("Sort by {column}", "Ordenar por {column}"),
    ("Live", "En vivo"),
    ("Connecting…", "Conectando…"),
    ("Refresh", "Actualizar"),
    ("Server", "Servidor"),
    ("Online", "Conectado"),
    ("Reconnecting…", "Reconectando…"),
    ("Devices", "Dispositivos"),
    ("Books with progress", "Libros con progreso"),
    ("Last activity", "Última actividad"),
    ("Every app or browser that saved a reading position", "Cada app o navegador que ha guardado una posición de lectura"),
    ("Recent activity", "Actividad reciente"),
    ("Updates live", "Se actualiza en vivo"),
    ("Connect your apps", "Conecta tus apps"),
    ("Reading apps (OPDS)", "Apps de lectura (OPDS)"),
    ("Add this catalog in KOReader, Moon+ Reader, Thorium, Panels or any OPDS app.", "Añade este catálogo en KOReader, Moon+ Reader, Thorium, Panels o cualquier app OPDS."),
    ("Copy address", "Copiar dirección"),
            ("User: {name}", "Usuario: {name}"),
        ("OPDShelf for KOReader", "OPDShelf para KOReader"),
    ("Live position sync and your library with covers, inside KOReader.", "Sincronización de la posición en vivo y tu biblioteca con portadas, dentro de KOReader."),
    ("Download the plugin", "Descargar el plugin"),
    ("Unzip it into KOReader's {folder} folder (on a Kobo, {path}).", "Descomprímelo en la carpeta {folder} de KOReader (en un Kobo, {path})."),
    ("Restart KOReader and open Tools › OPDShelf.", "Reinicia KOReader y abre Herramientas › OPDShelf."),
    ("Enter this server's address and your OPDShelf user:", "Escribe la dirección de este servidor y tu usuario de OPDShelf:"),
    ("Reads offline and syncs your position with every other device.", "Lee sin conexión y sincroniza tu posición con el resto de dispositivos."),
    ("Download the app", "Descargar la app"),
    ("Open the APK on the phone and allow installing apps from the browser if Android asks.", "Abre el APK en el móvil y permite instalar apps desde el navegador si Android te lo pide."),
    ("The app is downloaded from this server.", "La app se descarga desde este servidor."),
    (
        "This server is on the beta channel, so it offers the beta builds.",
        "Este servidor está en el canal beta, así que ofrece las versiones beta.",
    ),
    ("The app and the plugin are downloaded from the latest release on GitHub, so they stay up to date without updating the server.", "La app y el plugin se descargan de la última versión publicada en GitHub, así que están al día sin actualizar el servidor."),
    ("OPDShelf for Android", "OPDShelf para Android"),
    ("Use this server address when the app asks for it:", "Usa esta dirección de servidor cuando la app te la pida:"),
    ("This browser", "Este navegador"),
    ("Books read here sync with your other devices.", "Lo que leas aquí se sincroniza con tus otros dispositivos."),
    ("A book that is not in the library", "Un libro que no está en la biblioteca"),
    ("Connected now", "Conectado ahora"),
    ("Active now", "Activo ahora"),
    ("Never synced", "Nunca ha sincronizado"),
    ("Last seen {when}", "Visto por última vez {when}"),
    ("No devices yet", "Aún no hay dispositivos"),
    ("Open a book here, in KOReader or in the Android app and it will show up.", "Abre un libro aquí, en KOReader o en la app de Android y aparecerá."),
    ("Unknown device", "Dispositivo desconocido"),
    ("this browser", "este navegador"),
    ("Forget this device", "Olvidar este dispositivo"),
    ("1 book", "1 libro"),
    ("{n} books", "{n} libros"),
    ("1 position", "1 posición"),
    ("{n} positions", "{n} posiciones"),
    ("{n} connections", "{n} conexiones"),
    ("set by hand", "marcada a mano"),
    ("set a point in", "marcó un punto en"),
    ("read", "leyó"),
    ("to {pct}%", "hasta el {pct}%"),
    ("Nothing read yet.", "Todavía no se ha leído nada."),
    ("1 connected now", "1 conectado ahora"),
    ("{n} connected now", "{n} conectados ahora"),
    ("1 active in the last minutes", "1 activo en los últimos minutos"),
    ("{n} active in the last minutes", "{n} activos en los últimos minutos"),
    ("None connected now", "Ninguno conectado ahora"),
    ("{n} in progress", "{n} a medias"),
    ("No activity yet", "Sin actividad todavía"),
    ("Clocks agree", "Los relojes coinciden"),
    ("Cannot load the devices.", "No se pueden cargar los dispositivos."),
    ("Forget {device}?", "¿Olvidar {device}?"),
    ("Its saved positions are removed. Other devices keep theirs, and it shows up again if it syncs later.", "Se borran sus posiciones guardadas. Los demás dispositivos conservan las suyas, y volverá a aparecer si sincroniza más adelante."),
    ("Forget", "Olvidar"),
    ("{device} forgotten", "{device} olvidado"),
    ("Cannot forget the device: {reason}", "No se puede olvidar el dispositivo: {reason}"),
    ("This device's clock is {n} min off", "El reloj de este dispositivo va {n} min desfasado"),
    ("All books", "Todos los libros"),
    ("Reading", "Leyendo"),
    ("Finished", "Terminados"),
    ("Added this week", "Añadidos esta semana"),
    ("Missing details", "Faltan datos"),
    ("Sync", "Sincronización"),
    ("Folders", "Carpetas"),
    ("Formats", "Formatos"),
    ("OPDS catalog for reader apps", "Catálogo OPDS para apps de lectura"),
    ("Light or dark", "Claro u oscuro"),
    ("Language", "Idioma"),
    ("Sign out", "Salir"),
    ("Search", "Buscar"),
    ("Search title, author or file", "Buscar título, autor o archivo"),
    ("View", "Vista"),
    ("List", "Lista"),
    ("Covers", "Portadas"),
    ("Upload", "Subir"),
    ("Dismiss", "Cerrar"),
    ("Folder (empty for root)", "Carpeta (vacía para la raíz)"),
    ("Destination folder", "Carpeta de destino"),
    ("Move", "Mover"),
    ("Delete", "Borrar"),
    ("Clear selection", "Quitar selección"),
    ("Continue reading", "Seguir leyendo"),
    ("See all", "Ver todos"),
    ("Select all shown books", "Seleccionar todos los libros mostrados"),
    ("Title", "Título"),
    ("Folder", "Carpeta"),
    ("Format", "Formato"),
    ("Progress", "Progreso"),
    ("Size", "Tamaño"),
    ("Added", "Añadido"),
    ("Select {title}", "Seleccionar {title}"),
    ("No author", "Sin autor"),
    ("No cover", "Sin portada"),
    ("Read", "Leer"),
    ("No books match", "Ningún libro coincide"),
    ("Try another word or shelf.", "Prueba con otra palabra o estantería."),
    ("No books yet", "Aún no hay libros"),
    ("Drop EPUB, PDF or comic files here, or use Upload.", "Suelta aquí archivos EPUB, PDF o cómics, o usa Subir."),
    ("Upload books", "Subir libros"),
    ("Drop files anywhere to upload them to", "Suelta archivos en cualquier sitio para subirlos a"),
    ("the library root", "la raíz de la biblioteca"),
    ("Book", "Libro"),
    ("empty for the root", "vacía para la raíz"),
    ("Choose files or drop them here", "Elige archivos o suéltalos aquí"),
    ("Cancel", "Cancelar"),
    ("Rename or move", "Renombrar o mover"),
    ("File name", "Nombre del archivo"),
    ("Save", "Guardar"),
    ("Delete this book?", "¿Borrar este libro?"),
    ("{title} will be removed from the disk. This cannot be undone.", "{title} se borrará del disco. No se puede deshacer."),
    ("The files will be removed from the disk. This cannot be undone.", "Los archivos se borrarán del disco. No se puede deshacer."),
    ("Drop to upload to {folder}", "Suelta para subir a {folder}"),
    ("Nothing in progress", "Nada a medias"),
    ("Books you start on any device show up here.", "Aquí aparecen los libros que empieces en cualquier dispositivo."),
    ("No finished books yet", "Aún no has terminado ningún libro"),
    ("Books read to the end on any device show up here.", "Aquí aparecen los libros leídos hasta el final en cualquier dispositivo."),
    ("Done", "Leído"),
    ("{pct}% on {device}, {when}", "{pct}% en {device}, {when}"),
    ("another device", "otro dispositivo"),
    ("{pct}% · {device}", "{pct}% · {device}"),
    ("Continue reading {title}", "Seguir leyendo {title}"),
    ("Sync - OPDShelf", "Sincronización - OPDShelf"),
    ("Statistics - OPDShelf", "Estadísticas - OPDShelf"),
    ("Statistics", "Estadísticas"),
    ("Today", "Hoy"),
    ("Streak", "Racha"),
    ("Last 30 days", "Últimos 30 días"),
    ("Finished this year", "Terminados este año"),
    ("Time read per day", "Tiempo de lectura al día"),
    ("Worked out from the positions your devices sync", "Calculado a partir de las posiciones que sincronizan tus dispositivos"),
    ("Books finished per month", "Libros terminados por mes"),
    ("Most read in the last 30 days", "Lo más leído en los últimos 30 días"),
    ("{n} min", "{n} min"),
    ("{h} h {m} min", "{h} h {m} min"),
    ("1 day", "1 día"),
    ("{n} days", "{n} días"),
    ("1 session", "1 sesión"),
    ("{n} sessions", "{n} sesiones"),
    ("Longest: {days}", "La más larga: {days}"),
    ("{time} a day on {days} read", "{time} al día en {days} de lectura"),
    ("{time} read in all", "{time} leídos en total"),
    ("Cannot load the statistics.", "No se pueden cargar las estadísticas."),
    ("Nothing read in the last 30 days.", "Nada leído en los últimos 30 días."),
    ("1 book selected", "1 libro seleccionado"),
    ("{n} books selected", "{n} libros seleccionados"),
    ("Delete 1 book?", "¿Borrar 1 libro?"),
    ("Delete {n} books?", "¿Borrar {n} libros?"),
    ("Discard your changes?", "¿Descartar los cambios?"),
    ("The changes to this book have not been saved.", "Los cambios de este libro no se han guardado."),
    ("Discard", "Descartar"),
    ("Cannot load the reading progress.", "No se puede cargar el progreso de lectura."),
    ("Not started on any device yet.", "Aún no se ha empezado en ningún dispositivo."),
    ("on {device}, {when}", "en {device}, {when}"),
    ("(this browser)", "(este navegador)"),
    ("Found on {source}", "Encontrado en {source}"),
    ("(no title)", "(sin título)"),
    ("Restore the details from the file?", "¿Recuperar los datos del archivo?"),
    ("Your changes to this book are discarded.", "Se descartan tus cambios en este libro."),
    ("Restore", "Recuperar"),
    ("Not a supported format: {files}", "Formato no admitido: {files}"),
    ("Uploading {name}…", "Subiendo {name}…"),
    ("Uploading {n} books ({size})…", "Subiendo {n} libros ({size})…"),
    ("Reading the new books…", "Leyendo los libros nuevos…"),
    ("The connection was lost.", "Se ha perdido la conexión."),
    ("Upload failed: {reason}", "No se ha podido subir: {reason}"),
    ("Copied", "Copiado"),
    ("Live: changes arrive at once", "En vivo: los cambios llegan al momento"),
    ("Contents", "Índice"),
    ("Smaller text", "Letra más pequeña"),
    ("Larger text", "Letra más grande"),
    ("Previous page", "Página anterior"),
    ("Next page", "Página siguiente"),
    ("Set as last position", "Marcar como última posición"),
    ("Loading…", "Cargando…"),
    ("This book has no table of contents.", "Este libro no tiene índice."),
    ("Last position set. Other devices will open the book here.", "Posición marcada. Los demás dispositivos abrirán el libro aquí."),
    ("Opened at the point set on {device}", "Abierto en el punto marcado en {device}"),
    ("Continuing from {device} ({pct}%)", "Continuando desde {device} ({pct}%)"),
    ("Cannot open this book: {reason}", "No se puede abrir este libro: {reason}"),
    ("Reading direction", "Sentido de lectura"),
    ("Page", "Página"),
    ("no images found", "no se han encontrado imágenes"),
    ("Continuing from {device} (page {page})", "Continuando desde {device} (página {page})"),
    ("Cannot open this comic: {reason}", "No se puede abrir este cómic: {reason}"),
    ("Zoom out", "Alejar"),
    ("Zoom in", "Acercar"),
    ("Download", "Descargar"),
    ("Cannot open this PDF: {reason}", "No se puede abrir este PDF: {reason}"),
    ("Details", "Detalles"),
    ("Edit", "Editar"),
    ("Delete book", "Borrar libro"),
    ("Close", "Cerrar"),
    ("Unknown author", "Autor desconocido"),
    ("Series", "Serie"),
    ("Publisher", "Editorial"),
    ("Date", "Fecha"),
    ("Subjects", "Temas"),
    ("File", "Archivo"),
    ("Edited in OPDShelf", "Editado en OPDShelf"),
    ("From the file", "Del archivo"),
    ("Edit details", "Editar datos"),
    ("Fills the form from Open Library or Google Books", "Rellena el formulario desde Open Library o Google Books"),
    ("Look up", "Buscar"),
    ("Fill in the form", "Rellenar el formulario"),
    ("Use this cover", "Usar esta portada"),
    ("Upload image", "Subir imagen"),
    ("Use the file's cover", "Usar la portada del archivo"),
    ("Authors", "Autores"),
    ("separate with ;", "separa con ;"),
    ("Number in series", "Número en la serie"),
    ("ISBN / identifier", "ISBN / identificador"),
    ("separate with ,", "separa con ,"),
    ("Description", "Descripción"),
    ("Saved by OPDShelf. The book file is not modified.", "Lo guarda OPDShelf. El archivo del libro no se modifica."),
    ("Unsaved changes", "Cambios sin guardar"),
    ("Sign in", "Entrar"),
    ("Your library, your reading positions and your devices, in one place.", "Tu biblioteca, tus posiciones de lectura y tus dispositivos, en un solo sitio."),
    ("Username", "Usuario"),
    ("Password", "Contraseña"),
    ("Show password", "Mostrar contraseña"),
    ("Caps Lock is on", "Bloq Mayús está activado"),
    ("Reading apps such as KOReader sign in with the same user and password.", "Las apps de lectura como KOReader entran con el mismo usuario y contraseña."),
    ("Signing in…", "Entrando…"),
    ("Back to the library", "Volver a la biblioteca"),
    ("Page colour", "Color de página"),
    ("Light page", "Página clara"),
    ("Gray page", "Página gris"),
    ("Dark page", "Página oscura"),
    ("Position saved on the server", "Posición guardada en el servidor"),
    ("Saving the position…", "Guardando la posición…"),
    ("Offline: the position is kept here and sent when the connection is back", "Sin conexión: la posición se guarda aquí y se envía al volver la conexión"),
    ("Synced and following other devices live", "Sincronizado y siguiendo a otros dispositivos en vivo"),
    ("Not synced: this book has no id", "Sin sincronizar: este libro no tiene identificador"),
    ("{device} set a point here", "{device} ha marcado un punto aquí"),
    ("{device} is at {pct}%", "{device} va por el {pct}%"),
    ("Go there", "Ir allí"),
    ("just now", "ahora mismo"),
    ("Sign in - OPDShelf", "Entrar - OPDShelf"),
    ("Edit {title} - OPDShelf", "Editar {title} - OPDShelf"),
    ("Invalid username or password", "Usuario o contraseña incorrectos"),
    ("Uploaded {n} book(s)", "{n} libro(s) subidos"),
    ("Deleted {n} book(s)", "{n} libro(s) borrados"),
    ("Moved {n} book(s) to the library root", "{n} libro(s) movidos a la raíz de la biblioteca"),
    ("Moved {n} book(s) to {folder}", "{n} libro(s) movidos a {folder}"),
    ("Deleted {name}", "Borrado {name}"),
    ("Renamed to {path}", "Renombrado a {path}"),
    ("Book not found", "No se encuentra el libro"),
    ("Invalid folder name", "Nombre de carpeta no válido"),
    ("Invalid name", "Nombre no válido"),
    ("Rename failed: {reason}", "No se ha podido renombrar: {reason}"),
    ("Delete failed: {reason}", "No se ha podido borrar: {reason}"),
    ("Upload of {name} failed: {reason}", "No se ha podido subir {name}: {reason}"),
    ("Cannot create folder: {reason}", "No se puede crear la carpeta: {reason}"),
    ("Saved", "Guardado"),
    ("Cannot save: {reason}", "No se puede guardar: {reason}"),
    ("Cannot download the cover: {reason}", "No se puede descargar la portada: {reason}"),
    ("Cannot save the cover: {reason}", "No se puede guardar la portada: {reason}"),
    ("Cannot reset: {reason}", "No se puede recuperar: {reason}"),
    ("Restored the file's metadata", "Recuperados los datos del archivo"),
    ("Not a supported format: {list}", "Formato no admitido: {list}"),
    // Highlights and notes
    ("1 highlight", "1 subrayado"),
    ("1 highlight, with a note", "1 subrayado, con nota"),
    ("(no text)", "(sin texto)"),
    ("Add a note", "Añadir una nota"),
    ("All", "Todos"),
    ("Colour", "Color"),
    ("Copy", "Copiar"),
    ("Copy as text", "Copiar como texto"),
    ("Edit note", "Editar la nota"),
    ("Export", "Exportar"),
    ("Go to the highlight", "Ir al subrayado"),
    ("Highlight", "Subrayado"),
    ("Highlight deleted", "Subrayado borrado"),
    ("Highlights", "Subrayados"),
    ("Highlights and notes", "Subrayados y notas"),
    ("In book order", "En el orden del libro"),
    ("JSON file", "Archivo JSON"),
    ("Last position", "Última posición"),
    ("Markdown file", "Archivo Markdown"),
    ("New note", "Nueva nota"),
    ("No chapter", "Sin capítulo"),
    ("No highlights yet", "Aún no hay subrayados"),
    ("Note", "Nota"),
    ("Nothing matches.", "Nada coincide."),
    ("Order", "Orden"),
    ("Search highlights and notes", "Buscar en subrayados y notas"),
    ("Select text while reading, on the web, in the app or in KOReader, to highlight it or add a note. They show up here on every device.", "Selecciona texto mientras lees, en la web, en la app o en KOReader, para subrayarlo o añadir una nota. Aparecen aquí en todos los dispositivos."),
    ("Selected text", "Texto seleccionado"),
    ("This book has no id, so its highlights cannot be synced.", "Este libro no tiene identificador, así que sus subrayados no se pueden sincronizar."),
    ("Undo", "Deshacer"),
    ("With a note", "Con nota"),
    ("Write a note…", "Escribe una nota…"),
    ("p. {n}", "pág. {n}"),
    ("{n} highlights · {m} with a note", "{n} subrayados · {m} con nota"),
    ("Yellow", "Amarillo"),
    ("Orange", "Naranja"),
    ("Red", "Rojo"),
    ("Purple", "Morado"),
    ("Blue", "Azul"),
    ("Cyan", "Cian"),
    ("Green", "Verde"),
    ("Olive", "Oliva"),
    ("Gray", "Gris"),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_language_the_browser_prefers() {
        let mut headers = HeaderMap::new();
        assert_eq!(Lang::from_headers(&headers), Lang::En);
        headers.insert(header::ACCEPT_LANGUAGE, "es-ES,es;q=0.9,en;q=0.8".parse().unwrap());
        assert_eq!(Lang::from_headers(&headers), Lang::Es);
        headers.insert(header::ACCEPT_LANGUAGE, "fr-FR,en;q=0.5,es;q=0.4".parse().unwrap());
        assert_eq!(Lang::from_headers(&headers), Lang::En);
        headers.insert(header::COOKIE, "opdshelf_lang=es".parse().unwrap());
        assert_eq!(Lang::from_headers(&headers), Lang::Es);
    }

    #[test]
    fn translates_messages_with_values() {
        assert_eq!(
            match_template("Moved {n} book(s) to {folder}", "Moved 3 book(s) to Comics/Saga"),
            Some(vec![("n".into(), "3".into()), ("folder".into(), "Comics/Saga".into())])
        );
        assert_eq!(match_template("Deleted {name}", "Uploaded x"), None);
        assert_eq!(Lang::Es.message("Uploaded 2 book(s)"), "2 libro(s) subidos");
        assert_eq!(Lang::Es.message("Something else"), "Something else");
        assert_eq!(Lang::En.message("Uploaded 2 book(s)"), "Uploaded 2 book(s)");
    }

    /// Every text used by the templates and scripts has its Spanish version.
    #[test]
    fn every_text_is_translated() {
        let mut missing = Vec::new();
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        for dir in ["views", "views/layouts", "static/reader", "static/app"] {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if matches!(path.extension().and_then(|e| e.to_str()), Some("hbs" | "js")) {
                    files.push(path);
                }
            }
        }
        for path in files {
            let source = std::fs::read_to_string(&path).unwrap();
            for key in texts(&source) {
                if !spanish().contains_key(key.as_str()) {
                    missing.push(format!("{}: {key}", path.display()));
                }
            }
        }
        assert!(missing.is_empty(), "untranslated:\n{}", missing.join("\n"));
    }

    /// `{{t "…"}}` in templates and `t('…')` / t("…") / t(`…`) in scripts.
    fn texts(source: &str) -> Vec<String> {
        let mut found = Vec::new();
        for opener in ["{{t \"", "t('", "t(\"", "t(`"] {
            let quote = opener.chars().last().unwrap();
            let mut rest = source;
            while let Some(at) = rest.find(opener) {
                let before = rest[..at].chars().last();
                rest = &rest[at + opener.len()..];
                // Skip names that only end in t, such as `split('`
                if opener != "{{t \"" && before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.') {
                    continue;
                }
                if let Some(end) = rest.find(quote) {
                    found.push(rest[..end].to_string());
                }
            }
        }
        found
    }
}
