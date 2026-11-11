--[[--
The plugin's own translations, since KOReader's catalog does not have its
texts. Spanish for now; any other language uses KOReader's gettext, which
shows the English text.

@module opdshelf_i18n
--]]

local gettext = require("gettext")

local es = {
    ["%1 · %2 %3"] = "%1 · %2 de %3",
    ["Settings"] = "Ajustes",
    ["Open at start and after a book"] = "Abrir al empezar y al cerrar un libro",
    ["Stop opening at start and after a book"] = "No abrir al empezar ni al cerrar un libro",
    ["Opens OPDShelf's home screen instead of the file browser when KOReader starts and after closing a book, and the file browser's home button goes there."] = "Abre la pantalla de inicio de OPDShelf en lugar del explorador de archivos al arrancar KOReader y al cerrar un libro, y el botón de la casa del explorador lleva a ella.",
    ["January"] = "enero", ["February"] = "febrero", ["March"] = "marzo", ["April"] = "abril",
    ["May"] = "mayo", ["June"] = "junio", ["July"] = "julio", ["August"] = "agosto",
    ["September"] = "septiembre", ["October"] = "octubre", ["November"] = "noviembre", ["December"] = "diciembre",
    ["%1 #%2"] = "%1 n.º %2",
    ["%1 % read"] = "%1 % leído",
    ["%1 %"] = "%1 %",
    ["%1 (this device)"] = "%1 (este dispositivo)",
    ["%1 books in your library"] = "%1 libros en tu biblioteca",
    ["%1 days ago"] = "hace %1 días",
    ["%1 downloaded."] = "%1 descargado.",
    ["%1 h ago"] = "hace %1 h",
    ["%1 is already on this device."] = "%1 ya está en este dispositivo.",
    ["%1 items"] = "%1 elementos",
    ["%1 min ago"] = "hace %1 min",
    ["%1+ items"] = "%1+ elementos",
    ["%1: %2, %3"] = "%1: %2, %3",
    ["A point to continue this book from was set on %1. Go there?"] = "En %1 se marcó un punto para seguir este libro. ¿Ir allí?",
    ["Ask"] = "Preguntar",
    ["Behind on %1. Go back there?"] = "Más atrás en %1. ¿Volver allí?",
    ["CONTINUE READING"] = "SEGUIR LEYENDO",
    ["Cancel"] = "Cancelar",
    ["Cannot reach the server. It will be sent later."] = "No se puede contactar con el servidor. Se enviará más tarde.",
    ["Cannot reach the server."] = "No se puede contactar con el servidor.",
    ["Cannot reach the server: %1"] = "No se puede contactar con el servidor: %1",
    ["Connected to OPDShelf."] = "Conectado a OPDShelf.",
    ["Connecting…"] = "Conectando…",
    ["Continuing from %1"] = "Siguiendo desde %1",
    ["Could not download the book: %1"] = "No se pudo descargar el libro: %1",
    ["Device name: %1"] = "Nombre del dispositivo: %1",
    ["Download again"] = "Volver a descargar",
    ["Download and read"] = "Descargar y leer",
    ["Download folder: %1"] = "Carpeta de descargas: %1",
    ["Download only"] = "Solo descargar",
    ["Download"] = "Descargar",
    ["Downloading %1 (%2)…"] = "Descargando %1 (%2)…",
    ["Downloading %1…"] = "Descargando %1…",
    ["Files"] = "Archivos",
    ["Find a book in your OPDShelf library and download it to start."] = "Busca un libro en tu biblioteca de OPDShelf y descárgalo para empezar.",
    ["Finished"] = "Terminado",
    ["Further ahead on %1. Go there?"] = "Más adelante en %1. ¿Ir allí?",
    ["Further on %1: %2 % (%3). Open it to continue from there."] = "Más adelante en %1: %2 % (%3). Ábrelo para seguir desde allí.",
    ["Go there"] = "Ir allí",
    ["Go to where a device left this book"] = "Ir a donde un dispositivo dejó este libro",
    ["Go to where another device left it"] = "Ir a donde lo dejó otro dispositivo",
    ["Go"] = "Ir",
    ["Here"] = "Aquí",
    ["History"] = "Historial",
    ["Home"] = "Inicio",
    ["Keeps a connection to the server while a book is open, so reading done on other devices arrives at once. Positions are always sent after each page turn."] = "Mantiene una conexión con el servidor mientras hay un libro abierto, para que lo leído en otros dispositivos llegue al momento. La posición se envía siempre después de cada página.",
    ["Language:"] = "Idioma:",
    ["Last position set. Other devices will open the book here."] = "Última posición marcada. Los demás dispositivos abrirán el libro aquí.",
    ["Later"] = "Más tarde",
    ["Library home"] = "Inicio de la biblioteca",
    ["Library"] = "Biblioteca",
    ["Live sync while reading"] = "Sincronizar en directo al leer",
    ["Loading…"] = "Cargando…",
    ["Name of this device on other devices"] = "Nombre de este dispositivo en los demás",
    ["No device has synced this book yet."] = "Ningún dispositivo ha sincronizado aún este libro.",
    ["Not connected: set up the server in the menu"] = "Sin conectar: configura el servidor en el menú",
    ["Nothing here."] = "No hay nada aquí.",
    ["Nothing read yet"] = "Aún no has leído nada",
    ["OPDShelf home"] = "Inicio de OPDShelf",
    ["OPDShelf library"] = "Biblioteca OPDShelf",
    ["OPDShelf now syncs reading positions by itself, live. Turn off automatic Progress sync so the two do not compete?"] = "OPDShelf ya sincroniza la posición por su cuenta, en directo. ¿Desactivar la sincronización automática de progreso para que no compitan?",
    ["OPDShelf server"] = "Servidor de OPDShelf",
    ["OPDShelf"] = "OPDShelf",
    ["OPDShelf: sync this book now"] = "OPDShelf: sincronizar este libro ahora",
    ["Offline: showing what is on this device"] = "Sin conexión: se muestra lo que hay en el dispositivo",
    ["On this device"] = "En este dispositivo",
    ["Open the library"] = "Abrir la biblioteca",
    ["Opens OPDShelf's home screen instead of the file browser when KOReader starts and after closing a book. Also in File browser → Settings → Start with."] = "Abre la pantalla de inicio de OPDShelf en lugar del explorador de archivos al arrancar KOReader y al cerrar un libro. También en Explorador de archivos → Ajustes → Empezar con.",
    ["Other books you open will show here."] = "Aquí aparecerán los demás libros que abras.",
    ["Other device"] = "Otro dispositivo",
    ["Page %1 of %2"] = "Página %1 de %2",
    ["Password"] = "Contraseña",
    ["Published:"] = "Publicado:",
    ["Publisher:"] = "Editorial:",
    ["Read now"] = "Leer ahora",
    ["Read"] = "Leer",
    ["Recently added to the library"] = "Novedades en la biblioteca",
    ["Recently added"] = "Novedades",
    ["Refresh"] = "Actualizar",
    ["Reload"] = "Recargar",
    ["Save"] = "Guardar",
    ["Saved. It will be sent when the device is online."] = "Guardado. Se enviará cuando haya conexión.",
    ["Search the library"] = "Buscar en la biblioteca",
    ["Search"] = "Buscar",
    ["Search: %1"] = "Búsqueda: %1",
    ["Search…"] = "Buscar…",
    ["See all"] = "Ver todo",
    ["Server address"] = "Dirección del servidor",
    ["Server and account"] = "Servidor y cuenta",
    ["Server: %1"] = "Servidor: %1",
    ["Set as last position"] = "Marcar como última posición",
    ["Set up your OPDShelf server in the menu to see your library here."] = "Configura tu servidor de OPDShelf en el menú para ver aquí tu biblioteca.",
    ["Show as list"] = "Ver como lista",
    ["Show covers as a grid"] = "Ver portadas en cuadrícula",
    ["Show the home screen at start"] = "Mostrar la pantalla de inicio al empezar",
    ["Start KOReader here"] = "Empezar KOReader aquí",
    ["Start KOReader in the file browser"] = "Empezar KOReader en el explorador de archivos",
    ["Start with: %1"] = "Empezar con: %1",
    ["Stay here"] = "Quedarse aquí",
    ["Subjects:"] = "Temas:",
    ["Sync this book now"] = "Sincronizar este libro ahora",
    ["The OPDShelf server did not accept the username or password. Check them in OPDShelf → Server and account."] = "El servidor de OPDShelf no aceptó el usuario o la contraseña. Revísalos en OPDShelf → Servidor y cuenta.",
    ["The library's newest books will show here."] = "Aquí aparecerán los últimos libros de la biblioteca.",
    ["The server answered %1."] = "El servidor respondió %1.",
    ["The server did not accept the username or password. Check them in OPDShelf → Server and account."] = "El servidor no aceptó el usuario o la contraseña. Revísalos en OPDShelf → Servidor y cuenta.",
    ["The server did not accept the username or password."] = "El servidor no aceptó el usuario o la contraseña.",
    ["This book has no download."] = "Este libro no tiene descarga.",
    ["This book is up to date."] = "Este libro está al día.",
    ["This book was read here and on another device since they last synced.\n\nHere: %1%2\n%3\n\nWhere do you want to continue?"] = "Este libro se ha leído aquí y en otro dispositivo desde la última sincronización.\n\nAquí: %1%2\n%3\n\n¿Dónde quieres seguir?",
    ["This does not look like an OPDShelf server (it answered %1)."] = "No parece un servidor de OPDShelf (respondió %1).",
    ["This is not a library feed: %1"] = "Esto no es un catálogo de biblioteca: %1",
    ["Title, author or series"] = "Título, autor o serie",
    ["Turn off"] = "Desactivar",
    ["Username"] = "Usuario",
    ["When another device is behind"] = "Cuando otro dispositivo va por detrás",
    ["When another device is further ahead"] = "Cuando otro dispositivo va más adelante",
    ["another device"] = "otro dispositivo",
    ["just now"] = "ahora mismo",
    ["✓ downloaded"] = "✓ descargado",
    ["✓ read %1 %"] = "✓ leído %1 %",
}

local function language()
    local lang = type(gettext) == "table" and gettext.current_lang
    if (not lang or lang == "C") and G_reader_settings then
        lang = G_reader_settings:readSetting("language")
    end
    return lang or ""
end

local function translate(text)
    if language():match("^es") then
        return es[text] or gettext(text)
    end
    return gettext(text)
end

-- For the tests: which texts have a translation
local M = setmetatable({ es = es }, { __call = function(_self, text) return translate(text) end })
return M
