// Draws the sizes an .iconset folder wants of an SVG, given the folder and the
// SVG. `scripts/make-macos-icon` explains the tile; run it rather than this.

import AppKit

guard CommandLine.arguments.count == 3 else {
    FileHandle.standardError.write("usage: macos-icon.swift ICONSET SVG\n".data(using: .utf8)!)
    exit(2)
}
let folder = URL(fileURLWithPath: CommandLine.arguments[1])
let source = URL(fileURLWithPath: CommandLine.arguments[2])
guard let glyph = NSImage(contentsOf: source) else {
    FileHandle.standardError.write("could not read \(source.path)\n".data(using: .utf8)!)
    exit(1)
}

// Each side of the iconset, at 1x and 2x, as AppKit names them.
let sides: [(name: String, side: Int)] = [
    ("icon_16x16", 16), ("icon_16x16@2x", 32),
    ("icon_32x32", 32), ("icon_32x32@2x", 64),
    ("icon_128x128", 128), ("icon_128x128@2x", 256),
    ("icon_256x256", 256), ("icon_256x256@2x", 512),
    ("icon_512x512", 512), ("icon_512x512@2x", 1024),
]

/// Yapfast's own greys, from ui/theme.slint.
let top = NSColor(srgbRed: 0x1e / 255, green: 0x1f / 255, blue: 0x24 / 255, alpha: 1)
let bottom = NSColor(srgbRed: 0x11 / 255, green: 0x12 / 255, blue: 0x15 / 255, alpha: 1)

/// The tile's paint: the rail's grey below, the panel's above, as an angle of
/// 90 degrees runs from the first colour to the second.
func paint() -> NSGradient {
    NSGradient(starting: bottom, ending: top)!
}

for entry in sides {
    let side = entry.side
    guard let picture = NSBitmapImageRep(
        bitmapDataPlanes: nil, pixelsWide: side, pixelsHigh: side, bitsPerSample: 8,
        samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
        colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
    ) else {
        fatalError("no bitmap for \(side)")
    }
    picture.size = NSSize(width: side, height: side)
    NSGraphicsContext.saveGraphicsState()
    NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: picture)
    NSGraphicsContext.current?.imageInterpolation = .high

    // The tile takes 824 of 1024, as Apple's icon grid has it, with its
    // corner radius. Nothing else of the square is drawn, so it stays clear.
    let tile = CGFloat(side) * 0.8047
    let inset = (CGFloat(side) - tile) / 2
    let rect = NSRect(x: inset, y: inset, width: tile, height: tile)
    let corner = tile * 0.2187
    let path = NSBezierPath(roundedRect: rect, xRadius: corner, yRadius: corner)
    paint().draw(in: path, angle: 90)

    // The glyph fills most of the tile, with its own aspect kept.
    let mark = tile * 0.74
    let box = NSRect(
        x: (CGFloat(side) - mark) / 2, y: (CGFloat(side) - mark) / 2, width: mark, height: mark)
    glyph.draw(in: box)

    NSGraphicsContext.restoreGraphicsState()
    guard let png = picture.representation(using: .png, properties: [:]) else {
        fatalError("no PNG for \(side)")
    }
    try! png.write(to: folder.appendingPathComponent("\(entry.name).png"))
}
