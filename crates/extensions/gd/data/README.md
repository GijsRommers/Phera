# GD bitmap fonts

The five bitmaps are packed from `ext/gd/libgd/gdfont{t,s,mb,l,g}.c` in
php-src `php-8.5.13`. Each file stores the 256 original glyphs in byte order,
one bit per pixel, with consecutive bits in row order starting at the
least significant bit. Tiny, large and giant credit Libor Škarvada for
the original BDF font. Jan Pazdziora generated the libgd font tables.

The [libgd license](LICENSE-libgd.txt) covers the imported bitmaps and the
GD drawing algorithms adapted for this extension.
