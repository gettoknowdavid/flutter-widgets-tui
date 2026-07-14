import 'dart:io';

import 'model.dart';

/// Everything extracted from a single `///` doc comment block.
///
/// Deliberately does NOT depend on `package:dartdoc`'s internal (unstable,
/// version-pinned) model — no `dartdoc_session.dart` exists in this tool.
/// `{@macro ...}` and `{@template ...}` references are stripped, not
/// expanded, and `[CrossReference]` brackets are flattened to plain text
/// rather than resolved against dartdoc's real symbol table. This is a
/// deliberate scope boundary, not an oversight: expanding macros correctly
/// requires walking dartdoc's `PackageGraph`, which is exactly the
/// unstable API surface the project's own README warns to isolate and
/// re-verify on every version bump. If macro expansion becomes valuable
/// enough to justify that maintenance cost, it belongs in a new
/// `dartdoc_session.dart` that this parser calls into — not inline here.
class ParsedDoc {
  final String summary;
  final String overviewMarkdown;
  final List<CodeSampleRecord> codeSamples;
  final List<String> youtubeUrls;
  final String? relatedWidgetNameGuess;
  final bool hadUnresolvedMacros;

  ParsedDoc({
    required this.summary,
    required this.overviewMarkdown,
    required this.codeSamples,
    required this.youtubeUrls,
    required this.relatedWidgetNameGuess,
    required this.hadUnresolvedMacros,
  });
}

final RegExp _snippetToolRe = RegExp(
  r'\{@tool snippet\}(.*?)\{@end-tool\}',
  dotAll: true,
);
final RegExp _dartpadToolRe = RegExp(
  r'\{@tool dartpad(?:[^}]*)\}(.*?)\{@end-tool\}',
  dotAll: true,
);
final RegExp _codeFenceRe = RegExp(r'```dart\s*\n(.*?)```', dotAll: true);
final RegExp _seeCodeInRe = RegExp(r'\*\*\s*See code in ([^\*]+?)\s*\*\*');
final RegExp _youtubeRe = RegExp(r'\{@youtube\s+\d+\s+\d+\s+(\S+)\}');
final RegExp _macroOrTemplateRe = RegExp(
  r'\{@(macro|template|endtemplate)\b[^}]*\}',
);
// Catch-all for other single-line {@...} directives not special-cased
// above (e.g. {@animation ...}, {@inject-html}). NOTE: this only matches
// single-line/no-nested-brace directives — a multi-line {@template}...
// {@endtemplate} span whose body itself contains a '}' before the real
// close would not be fully caught by this fallback alone (the dedicated
// _macroOrTemplateRe above handles the common case; this is defense in
// depth for the long tail, with a known, accepted gap for adversarial
// nesting that doesn't occur in practice in Flutter's own doc comments).
final RegExp _genericDirectiveRe = RegExp(r'\{@[a-zA-Z\-]+[^}]*\}');
final RegExp _bracketRefRe = RegExp(r'\[([A-Za-z_][A-Za-z0-9_\.]*)\]');
final RegExp _seeAlsoHeaderRe = RegExp(r'see also\s*:', caseSensitive: false);

String _stripSlashes(String raw) {
  final lines = raw.split('\n').map((line) {
    final trimmed = line.trimLeft();
    if (trimmed.startsWith('///')) {
      final withoutSlashes = trimmed.substring(3);
      return withoutSlashes.startsWith(' ')
          ? withoutSlashes.substring(1)
          : withoutSlashes;
    }
    return line;
  });
  return lines.join('\n');
}

/// Lightweight clean for a single member's (constructor/property/method)
/// doc comment: strip `///`, drop embedded tool/macro directives, flatten
/// bracket references to plain text. No code-sample/youtube/related-widget
/// extraction — those are widget-level (top-of-class doc) concerns only,
/// handled by [parseDoc] below.
String cleanShortDoc(String raw) {
  if (raw.trim().isEmpty) return '';
  var body = _stripSlashes(raw);
  body = body.replaceAll(_snippetToolRe, '');
  body = body.replaceAll(_dartpadToolRe, '');
  body = body.replaceAll(_youtubeRe, '');
  body = body.replaceAll(_macroOrTemplateRe, '');
  body = body.replaceAll(_genericDirectiveRe, '');
  body = body.replaceAllMapped(_bracketRefRe, (m) => m.group(1)!);
  body = body.replaceAll(RegExp(r'\n{3,}'), '\n\n');
  return body.trim();
}

/// Full widget-level doc comment parse.
///
/// [knownWidgetNames] is used only for the best-effort "See also:"
/// related-widget guess — pass the full set of extracted widget names so
/// a bracketed reference like `[GridView]` only becomes a guess if
/// `GridView` is something this run actually catalogued (avoids guessing
/// at names that turn out to be typos, deprecated/removed classes, or
/// non-widget types like `[ScrollController]`).
///
/// [flutterSrcRoot], if provided, is the *root* of the Flutter SDK
/// checkout (the directory containing both `packages/` and `examples/`) —
/// used to resolve `{@tool dartpad}` blocks' `** See code in
/// examples/api/... **` markers into actual source. If omitted, or the
/// referenced file doesn't exist, the resulting [CodeSampleRecord] still
/// carries `examplePath` but has empty `code`.
ParsedDoc parseDoc(
  String raw, {
  required Set<String> knownWidgetNames,
  String? flutterSrcRoot,
}) {
  if (raw.trim().isEmpty) {
    return ParsedDoc(
      summary: 'No summary available.',
      overviewMarkdown: '',
      codeSamples: const [],
      youtubeUrls: const [],
      relatedWidgetNameGuess: null,
      hadUnresolvedMacros: false,
    );
  }

  var body = _stripSlashes(raw);
  final codeSamples = <CodeSampleRecord>[];
  final youtubeUrls = <String>[];

  // --- {@tool snippet} -> inline ```dart code blocks ---------------------
  body = body.replaceAllMapped(_snippetToolRe, (m) {
    final block = m.group(1) ?? '';
    final codeMatch = _codeFenceRe.firstMatch(block);
    codeSamples.add(
      CodeSampleRecord(
        label: 'Usage example',
        kind: 'snippet',
        code: (codeMatch?.group(1) ?? '').trim(),
        examplePath: null,
      ),
    );
    return '';
  });

  // --- {@tool dartpad} -> "** See code in examples/api/... **" ----------
  // Modern Flutter source doesn't inline dartpad code; it references an
  // examples/api file. We resolve that file's contents here so the
  // catalog stays genuinely offline-usable (NFR-1) rather than pointing
  // at a path the TUI can't read.
  body = body.replaceAllMapped(_dartpadToolRe, (m) {
    final block = m.group(1) ?? '';
    final pathMatch = _seeCodeInRe.firstMatch(block);
    final examplePath = pathMatch?.group(1)?.trim();
    var code = '';
    if (examplePath != null && flutterSrcRoot != null) {
      final file = File('$flutterSrcRoot/examples/api/$examplePath');
      if (file.existsSync()) {
        code = file.readAsStringSync().trim();
      }
    }
    codeSamples.add(
      CodeSampleRecord(
        label: examplePath != null
            ? 'Interactive example (${examplePath.split('/').last})'
            : 'Interactive example',
        kind: 'dartpad',
        code: code,
        examplePath: examplePath,
      ),
    );
    return '';
  });

  // --- {@youtube W H url} -------------------------------------------------
  body = body.replaceAllMapped(_youtubeRe, (m) {
    youtubeUrls.add(m.group(1)!);
    return '';
  });

  // --- "See also:" bullet list -> best-effort related-widget guess ------
  // Deliberately best-effort: takes the FIRST bracketed reference under a
  // "See also:" header that matches a known widget name from this run.
  // This is a starting point for `related_widget_id` resolution on the
  // Rust side (Section 4.2's cross-reference two-pass insert), not a
  // guarantee of the "best" or most relevant related widget — a human
  // reviewing seed content can always override it.
  String? relatedGuess;
  final seeAlsoIdx = _seeAlsoHeaderRe.firstMatch(body)?.end;
  if (seeAlsoIdx != null) {
    final tail = body.substring(seeAlsoIdx);
    for (final m in _bracketRefRe.allMatches(tail)) {
      final candidate = m.group(1)!.split('.').first;
      if (knownWidgetNames.contains(candidate)) {
        relatedGuess = candidate;
        break;
      }
    }
  }

  // --- {@macro ...} / {@template ...}...{@endtemplate} -------------------
  final hadMacros = _macroOrTemplateRe.hasMatch(body);
  body = body.replaceAll(_macroOrTemplateRe, '');
  body = body.replaceAll(_genericDirectiveRe, '');

  // --- [CrossReference] -> plain text -------------------------------------
  body = body.replaceAllMapped(_bracketRefRe, (m) => m.group(1)!);

  // --- collapse blank-line runs left behind by removed blocks ------------
  body = body.replaceAll(RegExp(r'\n{3,}'), '\n\n').trim();

  return ParsedDoc(
    summary: _firstSentence(body),
    overviewMarkdown: body,
    codeSamples: codeSamples,
    youtubeUrls: youtubeUrls,
    relatedWidgetNameGuess: relatedGuess,
    hadUnresolvedMacros: hadMacros,
  );
}

String _firstSentence(String body) {
  final firstParagraph = body
      .split('\n\n')
      .firstWhere((p) => p.trim().isNotEmpty, orElse: () => '');
  final flat = firstParagraph.replaceAll('\n', ' ').trim();
  if (flat.isEmpty) return 'No summary available.';

  final periodIdx = flat.indexOf('. ');
  if (periodIdx != -1) {
    return flat.substring(0, periodIdx + 1).trim();
  }
  return flat.endsWith('.') ? flat : '$flat.';
}
