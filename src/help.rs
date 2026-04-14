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
  ~           Home directory\n\
  >           Follow symlink\n\
\n\
{}\n\
  a           Toggle hidden files\n\
  A           Toggle long format\n\
  o           Cycle sort (name/size/time/ext)\n\
  i           Invert sort order\n\
  w           Change pane width\n\
  B           Cycle border style\n\
  -           Toggle preview pane\n\
  _           Toggle image preview\n\
  b           Toggle bat/internal syntax\n\
\n\
{}\n\
  /           Search filenames\n\
  n/N         Next/prev search match\n\
  \\           Clear search\n\
  f           Filter by extension\n\
  F           Filter by regex\n\
  Ctrl-F      Clear filter\n\
  g           Grep file contents\n\
  L           Locate file\n\
  Ctrl-N      Navi cheatsheets\n\
  Ctrl-P      fzf fuzzy finder\n\
\n\
{}\n\
  m           Set bookmark\n\
  '           Jump to bookmark\n\
  M           Show all bookmarks\n\
  Ctrl-R      Recent files/dirs\n\
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
  E           Bulk rename\n\
  X           Compare (2 tagged files)\n\
  x           Extract archive\n\
  Z           Create archive\n\
  U           Undo last operation\n\
\n\
{}\n\
  :           Shell command mode\n\
  ;           Command history\n\
  @           Script evaluator\n\
  +           Add to interactive list\n\
\n\
{}\n\
  ]  /  [     New / close tab\n\
  J  /  K     Next / prev tab\n\
  1-9         Switch to tab\n\
  {{  /  }}     Rename / duplicate tab\n\
\n\
{}\n\
  S-DOWN/UP   Scroll right pane line\n\
  TAB/S-TAB   Scroll right pane page\n\
  ENTER       Refresh preview\n\
\n\
{}\n\
  Ctrl-D      Toggle trash on/off\n\
  D           Browse trash (E to empty)\n\
\n\
{}\n\
  C           Preferences editor\n\
  W           Save config to disk\n\
  R           Reload config\n\
  V           Plugin manager\n\
  I           AI describe file\n\
  Ctrl-A      AI chat\n\
  Ctrl-E      SSH browser\n\
  e           File properties\n\
  D           Git status\n\
  H           Hash directory\n\
  S           System info\n\
  y/Y         Copy path primary/clipboard\n\
  Ctrl-Y      Copy right pane content\n\
  r           Refresh directory\n\
  v           Show version\n\
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
            style::fg("Trash", 220),
            style::fg("Other", 220),
        );

        self.show_in_right(&help);
    }

    pub fn show_version(&mut self) {
        self.status.say(&format!(" pointer v{}", env!("CARGO_PKG_VERSION")));
    }
}
