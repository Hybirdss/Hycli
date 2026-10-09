#!/bin/sh
# Per-user installation of an extracted, verified Linux archive.
set -eu
source_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix="$HOME/.local"
data_home="${XDG_DATA_HOME:-$HOME/.local/share}"
if [ "$#" -gt 0 ]; then
  if [ "$#" -ne 2 ] || [ "$1" != "--prefix" ]; then printf 'Usage: ./install.sh [--prefix directory]\n' >&2; exit 2; fi
  mkdir -p "$2"
  prefix=$(CDPATH= cd -- "$2" && pwd)
  data_home="$prefix/share"
fi
install_dir="$prefix/lib/hycli"
applications="$data_home/applications"
icons="$data_home/icons/hicolor/256x256/apps"
mkdir -p "$install_dir" "$prefix/bin" "$applications" "$icons"
# Running engines must exit before binaries are replaced. Never delete saved accounts.
if [ -x "$install_dir/hycli" ]; then "$install_dir/hycli" stop; fi
for binary in hycli hycli-desktop; do
  install -m 755 "$source_dir/$binary" "$install_dir/.$binary.new"
  mv -f "$install_dir/.$binary.new" "$install_dir/$binary"
done
install -m 644 "$source_dir/LICENSE" "$install_dir/LICENSE"
install -m 644 "$source_dir/hycli.png" "$icons/hycli.png"
ln -sfn "$install_dir/hycli" "$prefix/bin/hycli"
# Desktop Exec quoting is not shell quoting; escape its reserved characters and field codes.
exec_path=$(printf '%s' "$install_dir/hycli-desktop" | sed 's/\\/\\\\\\\\/g; s/"/\\\\"/g; s/`/\\\\`/g; s/\$/\\\\$/g; s/%/%%/g')
cat > "$applications/io.github.hybirdss.Hycli.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Hycli
Comment=Websites, ready for AI
Exec="$exec_path"
Icon=hycli
Terminal=false
Categories=Development;Utility;
Actions=Quit;

[Desktop Action Quit]
Name=Quit Hycli
Exec="$exec_path" stop
EOF
if command -v update-desktop-database >/dev/null 2>&1; then update-desktop-database "$applications"; fi
printf 'Installed Hycli. Open it from your application menu. CLI: %s\n' "$prefix/bin/hycli"
