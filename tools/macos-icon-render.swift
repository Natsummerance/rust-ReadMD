// Small offline AppKit renderer. SVG remains the single source of geometry.
// Explicit output paths avoid sips' system temporary-directory writes.
import AppKit
import Foundation

guard CommandLine.arguments.count == 4,
      let size = Int(CommandLine.arguments[2]), (16...1024).contains(size),
      let image = NSImage(data: try Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))),
      let bitmap = NSBitmapImageRep(bitmapDataPlanes: nil, pixelsWide: size, pixelsHigh: size,
          bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
          colorSpaceName: .deviceRGB, bytesPerRow: size * 4, bitsPerPixel: 32),
      let context = NSGraphicsContext(bitmapImageRep: bitmap) else { exit(2) }
NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = context
context.imageInterpolation = .high
image.draw(in: NSRect(x: 0, y: 0, width: size, height: size), from: .zero,
           operation: .sourceOver, fraction: 1)
NSGraphicsContext.restoreGraphicsState()
guard let png = bitmap.representation(using: .png, properties: [:]) else { exit(3) }
try png.write(to: URL(fileURLWithPath: CommandLine.arguments[3]), options: .withoutOverwriting)
