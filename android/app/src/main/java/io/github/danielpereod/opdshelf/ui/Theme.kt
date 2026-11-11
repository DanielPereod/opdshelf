package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.ui.res.stringResource
import io.github.danielpereod.opdshelf.R
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Shapes
import androidx.compose.material3.Text
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.foundation.layout.heightIn
import androidx.compose.ui.graphics.luminance
import androidx.compose.ui.semantics.contentType
import androidx.compose.runtime.getValue
import androidx.compose.runtime.setValue
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.ui.semantics.heading
import androidx.compose.ui.semantics.semantics

/** The web interface's palette (static/styles/index.css). */
@Immutable
data class ShelfColors(
    val bg: Color,
    val pane: Color,
    val hover: Color,
    val line: Color,
    val lineStrong: Color,
    val fg: Color,
    val muted: Color,
    val soft: Color,
    val inverse: Color,
    /** Destructive actions: remove, disconnect. */
    val danger: Color,
)

private val Light = ShelfColors(
    bg = Color(0xFFFAFAFA), pane = Color(0xFFFFFFFF), hover = Color(0xFFF3F3F3),
    // lineStrong outlines fields and chips: 3:1 against white (WCAG 1.4.11)
    line = Color(0xFFEBEBEB), lineStrong = Color(0xFF8C8C8C), fg = Color(0xFF111111),
    muted = Color(0xFF666666), soft = Color(0xFFF1F1F1), inverse = Color(0xFFFFFFFF),
    danger = Color(0xFFB3261E),
)

private val Dark = ShelfColors(
    bg = Color(0xFF0B0B0B), pane = Color(0xFF121212), hover = Color(0xFF181818),
    line = Color(0xFF242424), lineStrong = Color(0xFF6E6E6E), fg = Color(0xFFECECEC),
    muted = Color(0xFF9A9A9A), soft = Color(0xFF1E1E1E), inverse = Color(0xFF0B0B0B),
    danger = Color(0xFFF2B8B5),
)

/** The reader's paper themes, so its bars sit on the same page as the book. */
val ReaderLight = Light.copy(bg = Color(0xFFFFFFFF))

val ReaderSepia = ShelfColors(
    bg = Color(0xFFFAF4E8), pane = Color(0xFFFAF4E8), hover = Color(0xFFF0E7D5),
    line = Color(0xFFE6DAC3), lineStrong = Color(0xFF928670), fg = Color(0xFF121212),
    muted = Color(0xFF5E5545), soft = Color(0xFFEFE5D0), inverse = Color(0xFFFAF4E8),
    danger = Color(0xFFB3261E),
)

val ReaderDark = Dark.copy(bg = Color(0xFF000000), pane = Color(0xFF0E0E0E), soft = Color(0xFF1C1C1C), inverse = Color(0xFF000000))

/** The palette for a reading theme: 0 light, 1 sepia, 2 dark. */
fun readerPalette(theme: Int) = when (theme) {
    1 -> ReaderSepia
    2 -> ReaderDark
    else -> ReaderLight
}

val ShelfColors.isDark: Boolean get() = bg.luminance() < 0.5f

val LocalShelf = staticCompositionLocalOf { Light }

/** Shortcut to the palette inside composables. */
val shelf: ShelfColors @Composable get() = LocalShelf.current

val Mono = FontFamily.Monospace

/** The appearance chosen in Settings (0 system, 1 light, 2 dark), read by every [AppTheme]. */
object AppearanceState {
    var mode by androidx.compose.runtime.mutableIntStateOf(0)
}

/**
 * Spacing on Material's 4 dp baseline grid, so every margin and gap is one of these.
 * Screen edges use [Space.edge]; tablets get more room through [contentPadding].
 */
object Space {
    val xs = 4.dp
    val s = 8.dp
    val m = 12.dp
    val l = 16.dp
    val xl = 24.dp
    val xxl = 32.dp
    val edge = 16.dp
}

/** The text sizes of the app, a reduced Material 3 type scale. */
object TextSize {
    val display = 28.sp
    val headline = 22.sp
    val title = 20.sp
    val subtitle = 16.sp
    val body = 15.sp
    val bodySmall = 14.sp
    val label = 13.sp
    val caption = 12.sp
    val overline = 11.sp
}

/** Wider than a phone in portrait: tablets, foldables open, phones in landscape. */
val isWide: Boolean
    @Composable get() = androidx.compose.ui.platform.LocalConfiguration.current.screenWidthDp >= 600

/** The narrowest a book of a grid gets: wider with large fonts, so titles still fit. */
val gridCellWidth: Dp
    @Composable get() {
        val scale = androidx.compose.ui.platform.LocalDensity.current.fontScale
        val base = if (isWide) 128.dp else 96.dp
        return base * maxOf(1f, scale * 0.75f)
    }

/** Text and forms stay readable on wide screens: about 75 characters per line at most. */
val ReadableWidth = 640.dp

/** Side margin of a screen: 16 dp on phones, 24 dp on tablets. */
val edgePadding: Dp
    @Composable get() = if (isWide) Space.xl else Space.edge

/**
 * The app's look. [palette] forces one, as the reader does with its paper theme;
 * otherwise it follows the appearance chosen in Settings, or the system's.
 */
@Composable
fun AppTheme(palette: ShelfColors? = null, content: @Composable () -> Unit) {
    val mode = AppearanceState.mode
    val c = palette ?: when (mode) {
        1 -> Light
        2 -> Dark
        else -> if (isSystemInDarkTheme()) Dark else Light
    }
    val base = if (c.isDark) darkColorScheme() else lightColorScheme()
    val scheme = base.copy(
        primary = c.fg, onPrimary = c.inverse,
        primaryContainer = c.soft, onPrimaryContainer = c.fg,
        secondary = c.fg, onSecondary = c.inverse,
        secondaryContainer = c.soft, onSecondaryContainer = c.fg,
        tertiary = c.fg, onTertiary = c.inverse,
        background = c.bg, onBackground = c.fg,
        surface = c.pane, onSurface = c.fg,
        surfaceVariant = c.soft, onSurfaceVariant = c.muted,
        surfaceContainerLowest = c.pane, surfaceContainerLow = c.pane, surfaceContainer = c.pane,
        surfaceContainerHigh = c.soft, surfaceContainerHighest = c.soft,
        inverseSurface = c.fg, inverseOnSurface = c.inverse, inversePrimary = c.inverse,
        outline = c.lineStrong, outlineVariant = c.line,
        error = c.danger, onError = c.inverse,
        surfaceTint = Color.Transparent,
    )
    val shapes = Shapes(
        extraSmall = RoundedCornerShape(4.dp), small = RoundedCornerShape(4.dp),
        medium = RoundedCornerShape(6.dp), large = RoundedCornerShape(8.dp), extraLarge = RoundedCornerShape(12.dp),
    )
    CompositionLocalProvider(LocalShelf provides c) {
        MaterialTheme(colorScheme = scheme, shapes = shapes, typography = tightTypography(), content = content)
    }
}

/** Material's type scale without its extra letter spacing, closer to the web's text. */
private fun tightTypography(): androidx.compose.material3.Typography {
    val t = androidx.compose.material3.Typography()
    fun TextStyle.tight() = copy(letterSpacing = 0.sp)
    return t.copy(
        bodyLarge = t.bodyLarge.tight(), bodyMedium = t.bodyMedium.tight(), bodySmall = t.bodySmall.tight(),
        labelLarge = t.labelLarge.tight(), labelMedium = t.labelMedium.tight(), labelSmall = t.labelSmall.tight(),
        titleMedium = t.titleMedium.tight(), titleSmall = t.titleSmall.tight(),
    )
}

// ---------------------------------------------------------------------------
// Pieces shared by the screens, styled like their web counterparts

/** The top bar: "All books 2" with a back arrow and actions, over a hairline. */
@Composable
fun Header(
    title: String,
    count: Int? = null,
    onBack: (() -> Unit)? = null,
    leading: (@Composable () -> Unit)? = null,
    actions: @Composable RowScope.() -> Unit = {},
) {
    Column(Modifier.background(shelf.pane)) {
        Row(
            Modifier.fillMaxWidth().heightIn(min = 64.dp).padding(start = if (onBack != null) 4.dp else edgePadding, end = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, stringResource(R.string.back), tint = shelf.fg) }
            }
            leading?.let {
                it()
                Spacer(Modifier.width(Space.m))
            }
            // Title and count take what the actions leave, so the actions always sit at the end
            // The count runs on with the title, so it follows it when a long name wraps
            val muted = shelf.muted
            val text = androidx.compose.ui.text.buildAnnotatedString {
                append(title)
                if (count != null) {
                    append("\u2002")
                    pushStyle(androidx.compose.ui.text.SpanStyle(color = muted, fontSize = TextSize.bodySmall, fontWeight = FontWeight.Normal))
                    append(count.toString())
                    pop()
                }
            }
            Text(
                text, color = shelf.fg, fontSize = TextSize.title, fontWeight = FontWeight.Bold, lineHeight = 26.sp,
                maxLines = 2, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f).semantics { heading() },
            )
            Row(verticalAlignment = Alignment.CenterVertically, content = actions)
        }
        HorizontalDivider(color = shelf.line)
    }
}

/** The web's search box: hairline border, magnifier, compact. */
@Composable
fun SearchBox(value: String, onChange: (String) -> Unit, placeholder: String, onSearch: () -> Unit, modifier: Modifier = Modifier) {
    BasicTextField(
        value = value,
        onValueChange = onChange,
        singleLine = true,
        textStyle = TextStyle(color = shelf.fg, fontSize = TextSize.subtitle),
        cursorBrush = SolidColor(shelf.fg),
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        keyboardActions = KeyboardActions(onSearch = { onSearch() }),
        modifier = modifier,
        decorationBox = { field ->
            Row(
                Modifier.heightIn(min = 48.dp).clip(RoundedCornerShape(6.dp)).background(shelf.pane)
                    .border(1.dp, shelf.lineStrong, RoundedCornerShape(6.dp)).padding(start = Space.m, end = Space.xs),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Default.Search, null, tint = shelf.muted, modifier = Modifier.size(20.dp))
                Spacer(Modifier.width(Space.m))
                Box(Modifier.weight(1f)) {
                    if (value.isEmpty()) Text(placeholder, color = shelf.muted, fontSize = TextSize.subtitle, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    field()
                }
                if (value.isNotEmpty()) {
                    IconButton(onClick = { onChange("") }) {
                        Icon(Icons.Default.Close, stringResource(R.string.clear_search), tint = shelf.muted, modifier = Modifier.size(20.dp))
                    }
                }
            }
        },
    )
}

/** Small uppercase tag with a hairline border, like the web's format column. */
@Composable
fun Badge(text: String) {
    Text(
        text.uppercase(), color = shelf.muted, fontSize = TextSize.overline, fontWeight = FontWeight.SemiBold, letterSpacing = 0.5.sp,
        lineHeight = 16.sp, maxLines = 1,
        modifier = Modifier.border(1.dp, shelf.lineStrong, RoundedCornerShape(4.dp)).padding(horizontal = 6.dp),
    )
}

/** The black button of the web (Upload, Sign in). */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, height: Dp = 40.dp) {
    Button(
        onClick = onClick, enabled = enabled, shape = RoundedCornerShape(6.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = shelf.fg, contentColor = shelf.inverse,
            disabledContainerColor = shelf.soft, disabledContentColor = shelf.muted,
        ),
        contentPadding = PaddingValues(horizontal = Space.l, vertical = Space.s),
        modifier = modifier.heightIn(min = height),
    ) { ButtonText(text) }
}

@Composable
fun SecondaryButton(
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    height: Dp = 48.dp,
    destructive: Boolean = false,
    icon: androidx.compose.ui.graphics.vector.ImageVector? = null,
) {
    val color = if (destructive) shelf.danger else shelf.fg
    OutlinedButton(
        onClick = onClick, shape = RoundedCornerShape(6.dp), border = BorderStroke(1.dp, if (destructive) shelf.danger else shelf.lineStrong),
        colors = ButtonDefaults.outlinedButtonColors(contentColor = color),
        contentPadding = PaddingValues(horizontal = Space.l, vertical = Space.s), modifier = modifier.heightIn(min = height),
    ) {
        if (icon != null) {
            Icon(icon, null, modifier = Modifier.size(18.dp))
            Spacer(Modifier.width(Space.s))
        }
        ButtonText(text)
    }
}

/** A button's label: wraps to a second line before it is cut, as large fonts need. */
@Composable
private fun ButtonText(text: String) {
    Text(
        text, fontWeight = FontWeight.SemiBold, fontSize = TextSize.bodySmall, lineHeight = 20.sp,
        maxLines = 2, overflow = TextOverflow.Ellipsis, textAlign = androidx.compose.ui.text.style.TextAlign.Center,
    )
}

/** A field with its label above, as in the web's sign-in form. */
@Composable
fun LabeledField(
    label: String,
    value: String,
    onChange: (String) -> Unit,
    placeholder: String? = null,
    help: String? = null,
    keyboardType: KeyboardType = KeyboardType.Text,
    visualTransformation: VisualTransformation = VisualTransformation.None,
    trailing: (@Composable () -> Unit)? = null,
    imeAction: ImeAction = ImeAction.Default,
    onImeAction: (() -> Unit)? = null,
    isError: Boolean = false,
    autofill: androidx.compose.ui.autofill.ContentType? = null,
    capitalization: androidx.compose.ui.text.input.KeyboardCapitalization = androidx.compose.ui.text.input.KeyboardCapitalization.None,
    autoCorrect: Boolean = true,
) {
    Column(verticalArrangement = Arrangement.spacedBy(Space.s)) {
        Text(label, color = shelf.fg, fontSize = TextSize.label, fontWeight = FontWeight.SemiBold)
        OutlinedTextField(
            value = value, onValueChange = onChange, singleLine = true,
            placeholder = placeholder?.let { { Text(it, color = shelf.muted) } },
            shape = RoundedCornerShape(6.dp),
            visualTransformation = visualTransformation,
            trailingIcon = trailing,
            keyboardOptions = KeyboardOptions(
                keyboardType = keyboardType, imeAction = imeAction,
                capitalization = capitalization, autoCorrectEnabled = autoCorrect,
            ),
            keyboardActions = KeyboardActions(onAny = { onImeAction?.invoke() ?: defaultKeyboardAction(imeAction) }),
            isError = isError,
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = shelf.fg, unfocusedBorderColor = shelf.lineStrong,
                focusedContainerColor = shelf.pane, unfocusedContainerColor = shelf.pane,
                cursorColor = shelf.fg, focusedTextColor = shelf.fg, unfocusedTextColor = shelf.fg,
                errorBorderColor = shelf.danger, errorContainerColor = shelf.pane, errorCursorColor = shelf.fg,
                errorTextColor = shelf.fg, errorTrailingIconColor = shelf.danger,
            ),
            modifier = Modifier.fillMaxWidth().then(
                if (autofill != null) Modifier.semantics { contentType = autofill } else Modifier,
            ),
        )
        if (help != null) Text(help, color = shelf.muted, fontSize = TextSize.caption, lineHeight = 16.sp)
    }
}

/** A thin black bar on a soft track: how far a book was read. */
@Composable
fun ReadBar(fraction: Float, modifier: Modifier = Modifier) {
    Box(modifier.height(4.dp).clip(RoundedCornerShape(2.dp)).background(shelf.lineStrong.copy(alpha = 0.6f))) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(fraction.coerceIn(0f, 1f)).background(shelf.fg))
    }
}

@Composable
fun Spinner(modifier: Modifier = Modifier, size: Dp = 22.dp, onDark: Boolean = false) {
    CircularProgressIndicator(
        color = if (onDark) shelf.inverse else shelf.fg,
        trackColor = if (onDark) shelf.fg else shelf.soft,
        strokeWidth = 2.dp, modifier = modifier.size(size),
    )
}

/** A choice among a few, like the web sidebar's sections: filled when selected. */
@Composable
fun Pill(text: String, selected: Boolean, fontFamily: FontFamily? = null, onClick: () -> Unit) {
    // 32 dp to the eye, like Material's chips, but 48 dp to the finger
    Box(
        Modifier.heightIn(min = 48.dp)
            .selectable(selected = selected, role = androidx.compose.ui.semantics.Role.Tab, onClick = onClick),
        contentAlignment = Alignment.Center,
    ) {
        Box(
            Modifier.heightIn(min = 32.dp).clip(RoundedCornerShape(16.dp))
                .background(if (selected) shelf.fg else shelf.pane)
                .border(1.dp, if (selected) shelf.fg else shelf.lineStrong, RoundedCornerShape(16.dp))
                .padding(horizontal = Space.l, vertical = 6.dp),
            contentAlignment = Alignment.Center,
        ) {
            Text(text, color = if (selected) shelf.inverse else shelf.fg, fontSize = TextSize.label, lineHeight = 18.sp, fontWeight = FontWeight.SemiBold, fontFamily = fontFamily, maxLines = 1)
        }
    }
}

/** Empty and error states: an icon in a soft circle, a title, a line of help and what to do next. */
@Composable
fun EmptyState(
    text: String,
    action: String? = null,
    onAction: () -> Unit = {},
    title: String? = null,
    icon: androidx.compose.ui.graphics.vector.ImageVector? = null,
) {
    Column(
        Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(Space.xxl),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Column(Modifier.widthIn(max = 400.dp), horizontalAlignment = Alignment.CenterHorizontally) {
            if (icon != null) {
                Box(Modifier.size(64.dp).clip(RoundedCornerShape(32.dp)).background(shelf.soft), contentAlignment = Alignment.Center) {
                    Icon(icon, null, tint = shelf.fg, modifier = Modifier.size(28.dp))
                }
                Spacer(Modifier.height(Space.l))
            }
            if (title != null) {
                Text(
                    title, color = shelf.fg, fontSize = TextSize.title, fontWeight = FontWeight.Bold, lineHeight = 26.sp,
                    textAlign = androidx.compose.ui.text.style.TextAlign.Center, modifier = Modifier.semantics { heading() },
                )
                Spacer(Modifier.height(Space.s))
            }
            Text(text, color = shelf.muted, fontSize = TextSize.bodySmall, lineHeight = 20.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
            if (action != null) {
                Spacer(Modifier.height(Space.xl))
                PrimaryButton(action, onAction, height = 48.dp)
            }
        }
    }
}

/** The logo, on a light tile in dark mode, where its navy would vanish. */
@Composable
fun Logo(size: Dp, modifier: Modifier = Modifier) {
    val dark = shelf.isDark
    Box(
        modifier.size(size).then(
            if (dark) Modifier.clip(RoundedCornerShape(size / 4)).background(Color(0xFFDCE3EE)).padding(size / 8) else Modifier,
        ),
        contentAlignment = Alignment.Center,
    ) {
        androidx.compose.foundation.Image(
            androidx.compose.ui.res.painterResource(R.drawable.logo), stringResource(R.string.logo),
            Modifier.fillMaxSize(),
        )
    }
}

/** One entry of a book's ⋮ menu. */
data class MenuAction(
    val label: String,
    val icon: androidx.compose.ui.graphics.vector.ImageVector,
    val destructive: Boolean = false,
    val onClick: () -> Unit,
)

/** The ⋮ button of a book and the menu it opens. */
@Composable
fun MoreMenu(
    description: String,
    actions: List<MenuAction>,
    modifier: Modifier = Modifier,
    onImage: Boolean = false,
) {
    var open by androidx.compose.runtime.remember { androidx.compose.runtime.mutableStateOf(false) }
    Box(modifier) {
        IconButton(onClick = { open = true }) {
            if (onImage) {
                Box(Modifier.size(28.dp).clip(RoundedCornerShape(14.dp)).background(Color(0x99000000)), contentAlignment = Alignment.Center) {
                    Icon(androidx.compose.material.icons.Icons.Default.MoreVert, description, tint = Color.White, modifier = Modifier.size(18.dp))
                }
            } else {
                Icon(androidx.compose.material.icons.Icons.Default.MoreVert, description, tint = shelf.muted)
            }
        }
        androidx.compose.material3.DropdownMenu(
            expanded = open, onDismissRequest = { open = false },
            containerColor = shelf.pane, shape = RoundedCornerShape(8.dp),
            border = BorderStroke(1.dp, shelf.line),
        ) {
            actions.forEach { action ->
                val color = if (action.destructive) shelf.danger else shelf.fg
                androidx.compose.material3.DropdownMenuItem(
                    text = { Text(action.label, color = color, fontSize = TextSize.bodySmall) },
                    leadingIcon = { Icon(action.icon, null, tint = color, modifier = Modifier.size(20.dp)) },
                    onClick = {
                        open = false
                        action.onClick()
                    },
                    modifier = Modifier.heightIn(min = 48.dp),
                )
            }
        }
    }
}

/** A dialog asking to confirm something that cannot be undone, its action in the danger color. */
@Composable
fun ConfirmDialog(title: String, text: String, confirm: String, onConfirm: () -> Unit, onDismiss: () -> Unit) {
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title, color = shelf.fg) },
        text = { Text(text, color = shelf.muted, lineHeight = 20.sp) },
        confirmButton = {
            androidx.compose.material3.TextButton(onClick = onConfirm) {
                Text(confirm, color = shelf.danger, fontWeight = FontWeight.SemiBold)
            }
        },
        dismissButton = {
            androidx.compose.material3.TextButton(onClick = onDismiss) {
                Text(stringResource(R.string.cancel), color = shelf.fg)
            }
        },
        containerColor = shelf.pane,
        shape = RoundedCornerShape(12.dp),
    )
}
