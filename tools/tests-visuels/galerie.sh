#!/usr/bin/env bash
# Galerie HTML de toutes les captures : shots/index.html
cd "$(dirname "$0")/shots"
{
    echo '<!doctype html><meta charset="utf-8"><title>HeroCraft : captures</title>'
    echo '<style>body{background:#16131c;color:#eee;font:15px sans-serif;margin:24px}h2{color:#c9a0ff;margin-top:40px}figure{display:inline-block;margin:6px;width:420px;vertical-align:top}img{width:100%;border-radius:6px}figcaption{font-size:13px;color:#aaa}</style>'
    echo "<h1>HeroCraft : captures du $(date '+%d/%m/%Y %H:%M')</h1>"
    for d in */; do
        d=${d%/}
        ls "$d"/*.png >/dev/null 2>&1 || continue
        echo "<h2>$d</h2>"
        for f in "$d"/*.png; do
            echo "<figure><a href=\"$f\"><img src=\"$f\" loading=\"lazy\"></a><figcaption>$(basename "$f" .png)</figcaption></figure>"
        done
    done
} > index.html
