# Washi

Washi is a Windows sticky-notes app: handwritten-style notes on taped paper, kept in an encrypted local database.

## Language

**Note**:
One piece of writing, shown as a Paper in its own window. Notes are opened, closed and deleted individually.
_Avoid_: Sticky, memo

**Paper**:
The tilted white sheet with washi tape that makes up a Note's visible body.
_Avoid_: Card, page

**Tilted editing**:
How a Note's text is edited: by default it stays on the Paper's tilted baseline while you type, with the caret and the selection tilted too. Turned off (Settings › General, "Keep text tilted while editing"), the text settles flat for editing and tilts again at rest. One setting for every Note.
_Avoid_: Slanted editing (slant is the italic style)

**Check item**:
A paragraph of a Note with a box in front of it. Pressing the box ticks it: the box gets a green tick and the text turns grey and struck through. Made with the toolbar's check button, Ctrl+Shift+C, or by typing `[] ` at the start of a paragraph. Stored as `- [ ] text` and `- [x] text`.
_Avoid_: Todo, task, checkbox (the box is only the item's marker)

**List item**:
A paragraph of a Note with a bullet in front of it. Made with the toolbar's list button (when Bulleted was picked last), its flyout, Ctrl+Shift+L, or by typing `- ` at the start of a paragraph. Stored as `- text`.
_Avoid_: Bullet (the bullet is only the item's marker)

**Numbered item**:
A paragraph of a Note with its number in front (`1.`, `2.` ...). The number is never typed or stored on the paragraph: each unbroken run of numbered items counts from 1. Made with the list button's flyout, the list button when Numbered was picked last, or by typing `1. ` or `1, ` at the start of a paragraph. Stored as `N. text`.
_Avoid_: Ordered list item, step

**List**:
An unbroken run of list items, numbered items and check items, in any mix. A list has one of two positions and always moves as a whole: Left, where its markers hang in the margin and its text is in line with body text, or Indented, one step further right. Tab moves it Indented, Shift+Tab (or Backspace at the very front of one of its items) moves it back. A numbered list whose widest number does not fit the margin keeps its text right of it. Stored with four spaces before each item's marker when Indented.
_Avoid_: Nested list, sub-list (a list has no levels)

**Flyout**:
The small popup above the toolbar's list button that offers Bulleted and Numbered. Opened by the notch in the button's corner, a right-click on the button, or holding it down.
_Avoid_: Popover (that is the color popover), dropdown (that is the Dropdown menu)

**Dropdown menu**:
The popup opened from a Note's menu button. It lists notes to jump to and holds the New, All notes and Settings actions.
_Avoid_: Context menu (that is the separate right-click menu), overflow menu

**Delete dialog**:
The confirmation dialog opened when deleting a Note, asking to keep or delete the note.
_Avoid_: Confirmation box, prompt, modal alert

**All notes**:
The view that lists every Note in a Sidebar beside the Note being edited.
_Avoid_: Note list, manager

**Sidebar**:
The strip that comes out of the left of a Note's Paper. All notes and the Settings page each use one.
_Avoid_: Side panel, left panel, drawer

**Theme**:
How a Sidebar's panel is drawn, chosen on the Appearance Tab: Default (slate), Transparent, Paper or Liquid glass. Both Sidebars share one unless "Same for both sidebars" is off.
_Avoid_: Skin, style (Font & Style is a different Tab)

**Settings page**:
The view that replaces a Note with app-wide preferences, organised into tabs and navigated through its Sidebar. Opened from the Dropdown menu's Settings button.
_Avoid_: Setting page, preferences, options window

**Tab**:
One section of the Settings page, shown on the Paper: General, Appearance, Font & Style, Shortcuts, Privacy.
_Avoid_: Page, section

**Layout**:
The Note's geometry at one Paper size: where every block starts, where each line wraps, and so where the caret, a click, a selection and a picture are. Rebuilt when the text, the Paper, the tokens or a picture's size changes; every question about where text is is answered from it.
_Avoid_: Text flow, wrap (wrapping is one step of building it)

**Font**:
A family Washi can draw text in. The chosen Font, at its Axis values, is the regular look of all text in Washi: every Note and every screen. Bold and italic in a Note are made by Washi on top of that look.
_Avoid_: Typeface, font family (in the UI and code, "family" is the name inside a Font)

**Default font**:
Shadows Into Light, bundled with Washi. Always installed, in use until another Font is chosen, and cannot be removed. Its only Axis is Slant (Washi leans it); with no slant its bundled faces are used unchanged.
_Avoid_: Built-in font, system font (that is the fallback for letters a Font lacks)

**Installed font**:
A Font the user added from a `.zip` (as Google Fonts gives it) or a `.ttf`, kept in the fonts folder of Washi's data folder and listed in the Font manager. Installing does not make it the Font in use; installing the same family again updates it and keeps its Axis values.
_Avoid_: Imported font, custom font, downloaded font

**Font in use**:
The one Font all text in Washi is drawn in: the Default font or an Installed font. Shown on the Font & Style Tab's Font row and marked in the Font manager. Removing it switches back to the Default font.
_Avoid_: Current font, active font (that is the name of the code that holds it), selected font

**Font manager**:
The taped slip beside the right of the Settings page's Paper, opened from the Font row's manage button on the Font & Style Tab. It lists the Fonts, picks the Font in use, removes Installed fonts, installs new ones (by browsing or dropping a file onto it) and holds the Font in use's Axis controls. It closes with Esc, the manage button, a change of Tab or closing Settings.
_Avoid_: Font picker, font list, fonts slip (design name only)

**Axis**:
One adjustable setting of the Font in use: Weight, Width or Slant, each with a slider and a number box. Each Font remembers its own values. A Font only offers the Weight and Width its file has ("Not in this font" otherwise); Slant is always offered, using the font's own slant axis or, without one, a lean Washi makes.
_Avoid_: Variation, style, property (Font size is not an Axis: it is one number for all Fonts)

**Face set**:
The five static faces Washi makes in memory from the Font in use at its Axis values: Regular, Bold, Italic, Bold Italic and a blank Caret face. They share one set of letter widths so a Note's Layout and caret stay exact in any Font. Made again whenever the Font or an Axis value changes; never written to disk.
_Avoid_: Baked font, font bundle, font family (a Face set is registered under a private family name of its own)
