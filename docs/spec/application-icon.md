# Application icon

The running window uses Bart's approved arcane book-and-anvil art, including
the window's taskbar icon where the operating system supports it. Its built-in
PNG is 256 by 256 pixels. Smaller sizes use the same art, shrunk.

Windows builds embed the approved multi-size ICO in the program file, so Explorer
and shortcuts use that icon. Other target platforms skip program-file resource
embedding and do not need a Windows resource compiler.

The Linux AppImage and .deb carry the same 256 by 256 arcane PNG as their
package icon. The package smoke test fails a Linux package whose icon is not
that PNG.
