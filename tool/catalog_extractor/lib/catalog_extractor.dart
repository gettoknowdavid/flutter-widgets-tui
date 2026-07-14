/// Extracts Flutter widget metadata from a local Flutter SDK checkout into
/// `catalog.json`. Import this one file from `bin/extract_catalog.dart` —
/// see README.md's "File map" section for what each part underneath does.
library;

export 'src/analyzer_session.dart';
export 'src/doc_comment_parser.dart';
export 'src/extractor.dart';
export 'src/json_writer.dart';
export 'src/member_extractor.dart';
export 'src/model.dart';
export 'src/widget_filter.dart';
