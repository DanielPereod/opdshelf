package io.github.danielpereod.opdshelf.ui

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
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
)

private val Light = ShelfColors(
    bg = Color(0xFFFAFAFA), pane = Color(0xFFFFFFFF), hover = Color(0xFFF3F3F3),
    line = Color(0xFFEBEBEB), lineStrong = Color(0xFFD4D4D4), fg = Color(0xFF111111),
    muted = Color(0xFF767676), soft = Color(0xFFF1F1F1), inverse = Color(0xFFFFFFFF),
)

private val Dark = ShelfColors(
    bg = Color(0xFF0B0B0B), pane = Color(0xFF0B0B0B), hover = Color(0xFF181818),
    line = Color(0xFF222222), lineStrong = Color(0xFF333333), fg = Color(0xFFECECEC),
    muted = Color(0xFF8B8B8B), soft = Color(0xFF1B1B1B), inverse = Color(0xFF0B0B0B),
)

val LocalShelf = staticCompositionLocalOf { Light }

/** Shortcut to the palette inside composables. */
val shelf: ShelfColors @Composable get() = LocalShelf.current

val Mono = FontFamily.Monospace

@Composable
fun AppTheme(content: @Composable () -> Unit) {
    val c = if (isSystemInDarkTheme()) Dark else Light
    val base = if (c == Dark) darkColorScheme() else lightColorScheme()
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
            Modifier.fillMaxWidth().height(56.dp).padding(start = if (onBack != null) 4.dp else 16.dp, end = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (onBack != null) {
                IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, null, tint = shelf.fg) }
            }
            leading?.let {
                it()
                Spacer(Modifier.width(10.dp))
            }
            Text(
                title, color = shelf.fg, fontSize = 18.sp, fontWeight = FontWeight.Bold,
                maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false),
            )
            if (count != null) {
                Spacer(Modifier.width(6.dp))
                Text(count.toString(), color = shelf.muted, fontSize = 13.sp)
            }
            Spacer(Modifier.weight(1f))
            actions()
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
        textStyle = TextStyle(color = shelf.fg, fontSize = 15.sp),
        cursorBrush = SolidColor(shelf.fg),
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
        keyboardActions = KeyboardActions(onSearch = { onSearch() }),
        modifier = modifier,
        decorationBox = { field ->
            Row(
                Modifier.height(40.dp).clip(RoundedCornerShape(4.dp)).background(shelf.pane)
                    .border(1.dp, shelf.lineStrong, RoundedCornerShape(4.dp)).padding(horizontal = 10.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Icon(Icons.Default.Search, null, tint = shelf.muted, modifier = Modifier.size(18.dp))
                Spacer(Modifier.width(8.dp))
                Box(Modifier.weight(1f)) {
                    if (value.isEmpty()) Text(placeholder, color = shelf.muted, fontSize = 15.sp)
                    field()
                }
                if (value.isNotEmpty()) {
                    Icon(
                        Icons.Default.Close, null, tint = shelf.muted,
                        modifier = Modifier.size(18.dp).clickable { onChange("") },
                    )
                }
            }
        },
    )
}

/** Small uppercase tag with a hairline border, like the web's format column. */
@Composable
fun Badge(text: String) {
    Text(
        text.uppercase(), color = shelf.muted, fontSize = 10.sp, fontWeight = FontWeight.Medium, letterSpacing = 0.5.sp,
        modifier = Modifier.border(1.dp, shelf.lineStrong, RoundedCornerShape(3.dp)).padding(horizontal = 5.dp, vertical = 1.dp),
    )
}

/** The black button of the web (Upload, Sign in). */
@Composable
fun PrimaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier, enabled: Boolean = true, height: Dp = 36.dp) {
    Button(
        onClick = onClick, enabled = enabled, shape = RoundedCornerShape(4.dp),
        colors = ButtonDefaults.buttonColors(
            containerColor = shelf.fg, contentColor = shelf.inverse,
            disabledContainerColor = shelf.soft, disabledContentColor = shelf.muted,
        ),
        contentPadding = PaddingValues(horizontal = 14.dp),
        modifier = modifier.height(height),
    ) { Text(text, fontWeight = FontWeight.SemiBold, fontSize = 14.sp) }
}

@Composable
fun SecondaryButton(text: String, onClick: () -> Unit, modifier: Modifier = Modifier) {
    OutlinedButton(
        onClick = onClick, shape = RoundedCornerShape(4.dp), border = BorderStroke(1.dp, shelf.lineStrong),
        colors = ButtonDefaults.outlinedButtonColors(contentColor = shelf.fg),
        contentPadding = PaddingValues(horizontal = 14.dp), modifier = modifier.height(36.dp),
    ) { Text(text, fontWeight = FontWeight.SemiBold, fontSize = 14.sp) }
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
) {
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, color = shelf.muted, fontSize = 12.sp, fontWeight = FontWeight.SemiBold)
        OutlinedTextField(
            value = value, onValueChange = onChange, singleLine = true,
            placeholder = placeholder?.let { { Text(it, color = shelf.muted) } },
            shape = RoundedCornerShape(4.dp),
            visualTransformation = visualTransformation,
            keyboardOptions = KeyboardOptions(keyboardType = keyboardType),
            colors = OutlinedTextFieldDefaults.colors(
                focusedBorderColor = shelf.fg, unfocusedBorderColor = shelf.lineStrong,
                focusedContainerColor = shelf.pane, unfocusedContainerColor = shelf.pane,
                cursorColor = shelf.fg, focusedTextColor = shelf.fg, unfocusedTextColor = shelf.fg,
            ),
            modifier = Modifier.fillMaxWidth(),
        )
        if (help != null) Text(help, color = shelf.muted, fontSize = 12.sp)
    }
}

/** A thin black bar on a soft track: how far a book was read. */
@Composable
fun ReadBar(fraction: Float, modifier: Modifier = Modifier) {
    Box(modifier.height(3.dp).clip(RoundedCornerShape(2.dp)).background(shelf.soft)) {
        Box(Modifier.fillMaxHeight().fillMaxWidth(fraction.coerceIn(0f, 1f)).background(shelf.fg))
    }
}

@Composable
fun Spinner(modifier: Modifier = Modifier, size: Dp = 22.dp) {
    CircularProgressIndicator(color = shelf.fg, trackColor = shelf.soft, strokeWidth = 2.dp, modifier = modifier.size(size))
}

/** Centered muted text with an optional action, for empty and error states. */
@Composable
fun EmptyState(text: String, action: String? = null, onAction: () -> Unit = {}) {
    Column(
        Modifier.fillMaxSize().padding(32.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text(text, color = shelf.muted, fontSize = 14.sp, textAlign = androidx.compose.ui.text.style.TextAlign.Center)
        if (action != null) {
            Spacer(Modifier.height(16.dp))
            SecondaryButton(action, onAction)
        }
    }
}
