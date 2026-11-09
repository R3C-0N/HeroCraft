import pathlib, sys, druide, necromancien, ingenieur
out=pathlib.Path(sys.argv[1]); out.mkdir(exist_ok=True)
for f in druide.ALL+necromancien.ALL+ingenieur.ALL:
    m=f(); m.export(out/f"{m.ident}.gltf", out/f"{m.ident}.png")
