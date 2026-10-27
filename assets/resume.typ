// Original résumé geometry. User text remains literal data, never evaluated code.
#let d = json("resume.json")
#let k = 72 / 72.27
#let s = d.style
#let base = s.font_pt * k * 1pt
#let baseline = calc.max(s.at("line_pt", default: 14.72), s.font_pt + 0.5) * k * 1pt
#let leading = baseline - base
#let blue = rgb("244a70")
#let muted = rgb("687581")
#let family = if sys.inputs.at("resume-font", default: "system") == "noto" { "Noto Sans SC" } else { "Microsoft YaHei" }
#set page(paper: "a4", margin: (left: s.margin_mm * 1mm, right: s.margin_mm * 1mm, top: s.at("top_mm", default: 6.5) * 1mm, bottom: s.at("bottom_mm", default: 3.0) * 1mm))
#set text(font: family, size: base, fill: rgb("333d48"), lang: "zh", top-edge: base * 0.88, bottom-edge: -base * 0.12, hyphenate: false, overhang: false)
#set par(justify: true, leading: leading, spacing: 0pt)
#set block(above: 0pt, below: 0pt)
#let label(value) = text(weight: 600, fill: blue, value)
#let rich(value, highlights: ()) = {
  let rest = value
  let result = []
  while rest != "" {
    let first = none
    let phrase = ""
    for h in highlights.filter(h => h != "") {
      let at = rest.position(h)
      if at != none and (first == none or at < first or (at == first and h.len() > phrase.len())) { first = at; phrase = h }
    }
    if first == none { result += text(rest); rest = "" }
    else {
      result += text(rest.slice(0, first))
      result += text(weight: 600, fill: black, phrase)
      rest = rest.slice(first + phrase.len())
    }
  }
  result
}
#let paragraph(value) = { block(width: 100%, value); v(leading, weak: false) }
#let description(value, highlights) = {
  let colon = value.position("：")
  if colon != none and colon < 24 { [#label(value.slice(0, colon + "：".len()))#rich(value.slice(colon + "：".len()), highlights: highlights)] }
  else { rich(value, highlights: highlights) }
}
#let sectionbar(value) = block(width: 100%, height: 18 * k * 1pt, fill: rgb("eaf0f7"), stroke: (left: 2.5 * k * 1pt + blue), inset: (left: 8 * k * 1pt, top: 3.4 * k * 1pt))[
  #text(size: 12.5 * k * 1pt, weight: 600, fill: blue, top-edge: 0.88em, bottom-edge: -0.12em, value)
]
#let projectbar(value) = block(width: 100%, height: 15.5 * k * 1pt, fill: rgb("eff6f5"), stroke: (left: 3 * k * 1pt + rgb("23766d")), inset: (left: 8 * k * 1pt, top: 3.76 * k * 1pt))[#label(value)]
#let contact(value) = {
  for (i, part) in value.split("|").enumerate() {
    if i > 0 { text(fill: muted, "  |  ") }
    let at = part.position("：")
    if at != none { text(fill: muted, part.slice(0, at + "：".len()).trim()); text(part.slice(at + "：".len()).trim()) }
    else { text(part.trim()) }
  }
}
#block(width: 100%, height: 23mm)[
  #place(top + center, dy: 6.751pt)[
    #set par(justify: false)
    #set text(top-edge: 0.88em, bottom-edge: -0.12em)
    #text(size: 22 * k * 1pt, weight: 600, fill: blue, d.name)#h(12 * k * 1pt)#text(size: 9.5 * k * 1pt, fill: muted, d.tagline)
  ]
  #place(top + center, dy: 35.293pt)[
    #set par(justify: false)
    #set text(size: 9.5 * k * 1pt, top-edge: 0.88em, bottom-edge: -0.12em)
    #contact(d.contact)
  ]
  #place(top + center, dy: 47.896pt)[
    #set par(justify: false)
    #set text(size: 9.5 * k * 1pt, top-edge: 0.88em, bottom-edge: -0.12em)
    #contact(d.location)
  ]
  #if s.show_photo and d.photo != "" { place(top + right, dy: 1.4225pt, image(d.photo, width: 16.5mm, height: 22mm, fit: "cover")) }
]
#v(7.944pt, weak: false)
#for (section-index, section) in d.sections.enumerate() {
  let skills = section.entries.any(e => e.kind == "skills") or section.title == "专业技能"
  if section-index > 0 { v(if skills { 1.7 * k * 1pt } else { 1.34 * k * 1pt }, weak: false) }
  sectionbar(section.title)
  v(if skills { 4.61 * k * 1pt } else { 4 * k * 1pt }, weak: false)
  for (entry-index, entry) in section.entries.enumerate() {
    let highlights = entry.at("highlights", default: ())
    if entry.heading != "" {
      if entry.kind == "project" {
        projectbar(entry.heading)
        v(2.98 * k * 1pt, weak: false)
      } else {
        block(width: 100%)[
          #set text(size: 11.2 * k * 1pt, top-edge: 0.88em, bottom-edge: -0.12em)
          #let pieces = entry.heading.split("|")
          #label(pieces.first().trim())#if pieces.len() > 1 { h(0.5em); text(size: base, fill: muted, "|"); h(0.5em); text(size: base, pieces.slice(1).join("|").trim()) }
          #h(1fr)#text(size: 9.5 * k * 1pt, fill: muted, entry.meta)
        ]
        v(if entry-index + 1 < section.entries.len() { 0.375 * k * 1pt } else { 2 * k * 1pt }, weak: false)
      }
    }
    let summaries = entry.summary.split("\n").filter(p => p != "")
    for p in summaries.filter(p => p.starts-with("项目成果：")) { paragraph(description(p, highlights)) }
    if entry.kind == "project" and entry.meta != "" { paragraph(text(size: 9.5 * k * 1pt, fill: muted, entry.meta.replace("・", "·"))) }
    for p in summaries.filter(p => not p.starts-with("项目成果：")) { paragraph(description(p, highlights)) }
    if entry.bullets.len() > 0 {
      if summaries.len() > 0 { v(4 * k * 1pt, weak: false) }
      let gap = if skills or entry.kind == "skills" { s.at("skills_gap_pt", default: 3) } else { s.at("project_gap_pt", default: 7) }
      for (i, bullet) in entry.bullets.enumerate() {
        grid(columns: (base * 1.3, 1fr), column-gutter: 0pt, align: (right, left), [#text("•")#h(2 * k * 1pt)], [#if bullet.title != "" { label(bullet.title + "：") }#rich(bullet.body, highlights: bullet.at("highlights", default: ()))])
        if i + 1 < entry.bullets.len() { v(leading + gap * k * 1pt, weak: false) }
        else { v(4.46 * k * 1pt, weak: false) }
      }
    }
  }
}
