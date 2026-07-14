import 'dart:convert';
import 'dart:io';

import 'model.dart';

/// Writes [output] to [path] as pretty-printed JSON.
///
/// Deliberately a single, tiny, dumb function — no streaming, no
/// incremental writes. `catalog.json` for even the full ~700-widget
/// corpus is well within the range where building the whole JSON string
/// in memory and writing it once is simpler and fast enough; nothing
/// downstream needs this file until the Rust `xtask seed-catalog` step
/// runs, so there's no latency pressure on writing it either.
Future<void> writeCatalogJson(CatalogOutput output, String path) async {
  final encoder = JsonEncoder.withIndent('  ');
  await File(path).writeAsString(encoder.convert(output.toJson()));
}
