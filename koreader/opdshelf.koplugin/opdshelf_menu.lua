--[[--
Shows items written for KOReader's menus (text or text_func, callback,
checked_func, enabled_func, sub_item_table) as a dialog of buttons, so the
home screen offers the very same settings as Tools › OPDShelf › Settings
from one list.

@module opdshelf_menu
--]]

local ButtonDialog = require("ui/widget/buttondialog")
local UIManager = require("ui/uimanager")

local Menu = {}

--- The label of an item, with a mark when it is ticked.
function Menu.label(item)
    local text = item.text_func and item.text_func() or item.text or ""
    if item.checked_func then
        local on = item.checked_func()
        if item.radio then
            text = (on and "● " or "○ ") .. text
        else
            text = (on and "☑ " or "☐ ") .. text
        end
    end
    if item.sub_item_table or item.sub_item_table_func then text = text .. " ›" end
    return text
end

--- Shows `items` as a dialog titled `title`. `extra` are rows of buttons
--- put first. `done()` runs after an item did its work, so the caller can
--- refresh what it shows.
function Menu.dialog(title, items, extra, done)
    local dialog
    local buttons = {}
    for _i, row in ipairs(extra or {}) do buttons[#buttons + 1] = row end
    for _i, item in ipairs(items) do
        local enabled = not item.enabled_func or item.enabled_func()
        buttons[#buttons + 1] = { {
            text = Menu.label(item),
            align = "left",
            enabled = enabled,
            callback = function()
                UIManager:close(dialog)
                local sub = item.sub_item_table or (item.sub_item_table_func and item.sub_item_table_func())
                if sub then
                    -- Choices go back to this dialog, as a submenu does
                    Menu.dialog(item.text_func and item.text_func() or item.text, sub, nil, function()
                        Menu.dialog(title, items, extra, done)
                    end)
                    return
                end
                if item.callback then item.callback() end
                if done then done(item) end
            end,
        } }
    end
    dialog = ButtonDialog:new{
        title = title,
        title_align = "center",
        shrink_unneeded_width = true,
        buttons = buttons,
    }
    UIManager:show(dialog)
    return dialog
end

return Menu
