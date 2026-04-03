use crate::app::App;
use crust::style;

impl App {
    pub fn show_help(&mut self) {
        let help = format!("{}\n\n\
{}\n\
  j/DOWN      Move down\n\
  k/UP        Move up\n\
  l/RIGHT/RET Enter directory / open file\n\
  h/LEFT      Go to parent directory\n\
  g/HOME      Go to top\n\
  G/END       Go to bottom\n\
  PgDN/Space  Page down\n\
  PgUP        Page up\n\
\n\
{}\n\
  a           Toggle hidden files\n\
  A           Toggle long format\n\
  o           Cycle sort (name/size/time/ext)\n\
  i           Invert sort order\n\
  w           Change pane width\n\
  B           Cycle border style\n\
  -           Toggle preview pane\n\
  b           Toggle bat syntax highlighting\n\
\n\
{}\n\
  /           Search filenames\n\
  n           Next search match\n\
  N           Previous search match\n\
  \\           Clear search\n\
  f           Filter by extension\n\
  F           Filter by regex\n\
  Ctrl-F      Clear filter\n\
\n\
{}\n\
  m           Set bookmark\n\
  '           Jump to bookmark\n\
  M           Show all bookmarks\n\
  ~           Go to home directory\n\
  >           Follow symlink\n\
\n\
{}\n\
  t           Tag/untag current item\n\
  T           Show tagged items\n\
  u           Clear all tags\n\
  Ctrl-T      Tag by pattern\n\
\n\
{}\n\
  p           Copy tagged/selected here\n\
  P           Move tagged/selected here\n\
  d           Delete tagged/selected\n\
  c           Rename current item\n\
  s           Create symlinks\n\
  =           Create directory\n\
  U           Undo last operation\n\
\n\
{}\n\
  :           Shell command mode\n\
  ;           Command history\n\
\n\
{}\n\
  ]           New tab\n\
  [           Close tab\n\
  J           Next tab\n\
  K           Previous tab\n\
  1-9         Switch to tab\n\
\n\
{}\n\
  Shift-UP/DN Scroll line\n\
  TAB/S-TAB   Scroll page\n\
\n\
{}\n\
  e           File properties\n\
  y           Copy path (primary)\n\
  Y           Copy path (clipboard)\n\
  r           Refresh\n\
  ?           This help\n\
  q           Quit (save state)\n\
  Q           Quit (no save)",
            style::bold("Pointer - Terminal File Manager"),
            style::fg("Navigation", 220),
            style::fg("View", 220),
            style::fg("Search & Filter", 220),
            style::fg("Marks", 220),
            style::fg("Tags", 220),
            style::fg("File Operations", 220),
            style::fg("Command", 220),
            style::fg("Tabs", 220),
            style::fg("Right Pane", 220),
            style::fg("Other", 220),
        );

        self.show_in_right(&help);
    }

    pub fn show_version(&mut self) {
        self.status.say(&format!(" pointer v{}", env!("CARGO_PKG_VERSION")));
    }
}
