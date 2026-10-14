import AppKit

func usage() -> Never {
    FileHandle.standardError.write(
        "usage: gen-icon.swift <out.png>\n       gen-icon.swift --art <in.png> <out.png>\n".data(using: .utf8)!
    )
    exit(1)
}

var args = Array(CommandLine.arguments.dropFirst())
var artPath: String?

if args.first == "--art" {
    args.removeFirst()
    guard args.count >= 2 else { usage() }
    artPath = args.removeFirst()
}
guard args.count == 1, let outPath = args.first else { usage() }

guard let rep = NSBitmapImageRep(
    bitmapDataPlanes: nil, pixelsWide: 1024, pixelsHigh: 1024,
    bitsPerSample: 8, samplesPerPixel: 4, hasAlpha: true, isPlanar: false,
    colorSpaceName: .deviceRGB, bytesPerRow: 0, bitsPerPixel: 0
) else {
    fatalError("failed to create bitmap rep")
}

NSGraphicsContext.saveGraphicsState()
NSGraphicsContext.current = NSGraphicsContext(bitmapImageRep: rep)
NSColor.clear.set()
NSBezierPath(rect: NSRect(x: 0, y: 0, width: 1024, height: 1024)).fill()

// Icon body: Big Sur grid — 824x824 at (100,100), continuous-ish corner radius 185.
let body = NSBezierPath(
    roundedRect: NSRect(x: 100, y: 100, width: 824, height: 824),
    xRadius: 185, yRadius: 185
)

let shadow = NSShadow()
shadow.shadowBlurRadius = 28
shadow.shadowOffset = NSSize(width: 0, height: -12)
shadow.shadowColor = NSColor.black.withAlphaComponent(0.28)

if let artPath {
    guard let art = NSImage(contentsOfFile: artPath),
          let artRep = art.representations.first,
          artRep.pixelsWide > 0, artRep.pixelsHigh > 0
    else {
        fatalError("failed to load artwork: \(artPath)")
    }

    // Soft shadow cast by the squircle body, then the artwork fills it exactly.
    shadow.set()
    NSColor(srgbRed: 0xd9 / 255.0, green: 0x53 / 255.0, blue: 0x1a / 255.0, alpha: 1).setFill()
    body.fill()
    NSShadow().set()

    let target = NSRect(x: 100, y: 100, width: 824, height: 824)
    body.addClip()

    // Aspect-fill: scale to cover the 824x824 rect, centered.
    let iw = CGFloat(artRep.pixelsWide)
    let ih = CGFloat(artRep.pixelsHigh)
    let scale = max(target.width / iw, target.height / ih)
    let w = iw * scale
    let h = ih * scale
    let drawRect = NSRect(x: target.midX - w / 2, y: target.midY - h / 2, width: w, height: h)
    art.draw(in: drawRect, from: .zero, operation: .copy, fraction: 1)

    // 1 px inner stroke along the same path for crisp edges (clipped → inner half only).
    NSColor.white.withAlphaComponent(0.12).setStroke()
    body.lineWidth = 2
    body.stroke()
} else {
    shadow.set()

    let top = NSColor(srgbRed: 0xd9 / 255.0, green: 0x53 / 255.0, blue: 0x1a / 255.0, alpha: 1)
    let bottom = NSColor(srgbRed: 0xb2 / 255.0, green: 0x3a / 255.0, blue: 0x0a / 255.0, alpha: 1)
    let gradient = NSGradient(starting: top, ending: bottom)!
    gradient.draw(in: body, angle: -90)

    NSShadow().set()

    // Mark: four round-capped vertical bars + dot, like an orange waveform cursor.
    NSColor(srgbRed: 0xff / 255.0, green: 0xfd / 255.0, blue: 0xf7 / 255.0, alpha: 1).set()
    let xCenters: [CGFloat] = [332, 440, 548, 656]
    let halfHeights: [CGFloat] = [64, 144, 196, 100]
    for (x, half) in zip(xCenters, halfHeights) {
        let line = NSBezierPath()
        line.lineWidth = 58
        line.lineCapStyle = .round
        line.move(to: NSPoint(x: x, y: 512 - half))
        line.line(to: NSPoint(x: x, y: 512 + half))
        line.stroke()
    }
    let dot = NSBezierPath(ovalIn: NSRect(x: 750 - 32, y: 512 - 32, width: 64, height: 64))
    dot.fill()
}

NSGraphicsContext.restoreGraphicsState()

guard let data = rep.representation(using: .png, properties: [:]) else {
    fatalError("failed to encode png")
}
try! data.write(to: URL(fileURLWithPath: outPath))
print("wrote \(outPath)")
