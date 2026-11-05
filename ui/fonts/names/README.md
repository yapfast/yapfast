The fonts Discord members can give their names, at the weights Discord's client
draws them in, each with its SIL Open Font License 1.1 as `OFL-*.txt`. Names in
a font that isn't here show in the UI's typeface.

Unmodified, byte for byte the files in the google/fonts repository's `ofl`
folder: Cherry Bomb One, Chicle, Kalam Bold, New Rocker and Zilla Slab SemiBold.
Chicle and New Rocker have Reserved Font Names, so these two must stay unmodified
to keep those names.

Static cuts of variable fonts, with no Reserved Font Name in their licenses, so
they keep their names. Each is one instance of the variable font with the variation
data removed, some with their letters scaled by a changed units-per-em, to match
the size Discord's client draws those names at:

- MuseoModerno: weight 500, 1086 units per em, from `MuseoModerno[wght].ttf` in
  google/fonts.
- Pixelify Sans: weight 500, from `PixelifySans[wght].ttf` in google/fonts.
- Playpen Sans: weight 700, 1100 units per em, from `PlaypenSans[wght].ttf` in
  google/fonts.
- Sinistre: weight 547, from `fonts/SinistreVF.ttf` in
  github.com/collletttivo/sinistre.

Yapfast Orbit is a modified Orbitron. Orbitron's license reserves its name, so
this cut of it has its own: its weight 700 instance from `Orbitron[wght].ttf` in
google/fonts, with the variation data removed, 1220 units per em, and its family
and PostScript names changed to Yapfast Orbit and YapfastOrbit-Bold. Its
copyright notice and `OFL-orbitron.txt` are Orbitron's, unchanged.
