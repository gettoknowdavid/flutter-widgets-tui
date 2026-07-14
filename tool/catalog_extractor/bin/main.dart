import 'dart:convert';
import 'dart:io';
import 'package:analyzer/dart/analysis/analysis_context_collection.dart';
import 'package:analyzer/dart/analysis/results.dart';
import 'package:analyzer/dart/element/element.dart';
import 'package:path/path.dart' as p;

void main(List<String> args) async {
  if (args.isEmpty) {
    print(
      'Usage: dart run catalog_extractor_unified.dart <PATH_TO_FLUTTER_SDK>',
    );
    return;
  }

  final flutterSdkPath = args[0];
  final flutterPackagePath = p.normalize(
    p.absolute(p.join(flutterSdkPath, 'packages', 'flutter')),
  );
  final libPath = p.join(flutterPackagePath, 'lib');

  // ---------------------------------------------------------------------------
  // STEP 1: Fetch Categories from Web
  // ---------------------------------------------------------------------------
  print(
    '🌐 Step 1: Dynamically discovering categories from docs.flutter.dev...',
  );
  final Map<String, Map<String, String>> webCategoryMap =
      await _fetchWebCategories();
  print('✅ Mapped ${webCategoryMap.length} widgets from the web catalog.\n');

  // ---------------------------------------------------------------------------
  // STEP 2: Analyze Local SDK via AST
  // ---------------------------------------------------------------------------
  print('🚀 Step 2: Analyzing Flutter SDK at: $flutterPackagePath');

  final contextCollection = AnalysisContextCollection(
    includedPaths: [flutterPackagePath],
  );
  final context = contextCollection.contextFor(flutterPackagePath);
  final session = context.currentSession;

  final Map<String, dynamic> sdkData = {
    'classes': [],
    'mixins': [],
    'enums': [],
    'extensions': [],
    'extensionTypes': [],
    'functions': [],
  };

  final Set<String> processedElements = {};

  final topLevelFiles = Directory(
    libPath,
  ).listSync().whereType<File>().where((f) => f.path.endsWith('.dart'));

  for (final file in topLevelFiles) {
    print('🔍 Analyzing entry point: ${p.basename(file.path)}');

    final result = await session.getResolvedLibrary(file.path);
    if (result is ResolvedLibraryResult) {
      final exportNamespace = result.element.exportNamespace;

      for (final element in exportNamespace.definedNames2.values) {
        if (element.name?.startsWith('_') ?? true) continue;

        final sourceUri = element.library?.uri.toString() ?? '';
        if (sourceUri.startsWith('dart:')) continue;
        if (sourceUri.startsWith('package:') &&
            !sourceUri.startsWith('package:flutter/'))
          continue;
        if (sourceUri.startsWith('file:') &&
            !sourceUri.contains('/packages/flutter/lib/'))
          continue;

        String? uniqueId;

        if (element is ClassElement) {
          uniqueId = 'class:${element.name}';
          if (!processedElements.contains(uniqueId)) {
            final classData = _extractClassLike(element);

            // --- INJECT CATEGORY DATA FOR WIDGETS ---
            if (classData['isWidget'] == true) {
              final widgetName = element.name;

              if (webCategoryMap.containsKey(widgetName)) {
                // Known web widget
                classData['top_level'] =
                    webCategoryMap[widgetName]!['top_level'];
                classData['sub_category'] =
                    webCategoryMap[widgetName]!['sub_category'];
              } else {
                // Undocumented / Internal SDK Widget
                if (sourceUri.contains('/material/')) {
                  classData['top_level'] = 'Design Systems';
                  classData['sub_category'] = 'Material Components';
                } else if (sourceUri.contains('/cupertino/')) {
                  classData['top_level'] = 'Design Systems';
                  classData['sub_category'] = 'Cupertino';
                } else {
                  classData['top_level'] = 'Base Widgets';
                  classData['sub_category'] = 'Miscellaneous';
                }
              }
            }
            // ----------------------------------------
            sdkData['classes'].add(classData);
          }
        } else if (element is MixinElement) {
          uniqueId = 'mixin:${element.name}';
          if (!processedElements.contains(uniqueId)) {
            sdkData['mixins'].add(_extractClassLike(element));
          }
        } else if (element is EnumElement) {
          uniqueId = 'enum:${element.name}';
          if (!processedElements.contains(uniqueId)) {
            sdkData['enums'].add(_extractEnum(element));
          }
        } else if (element is ExtensionElement) {
          uniqueId = 'extension:${element.name}';
          if (!processedElements.contains(uniqueId)) {
            sdkData['extensions'].add(_extractExtension(element));
          }
        } else if (element is ExtensionTypeElement) {
          uniqueId = 'extensionType:${element.name}';
          if (!processedElements.contains(uniqueId)) {
            sdkData['extensionTypes'].add(_extractExtensionType(element));
          }
        } else if (element is TopLevelFunctionElement) {
          uniqueId = 'function:${element.name}';
          if (!processedElements.contains(uniqueId)) {
            sdkData['functions'].add(_extractFunction(element));
          }
        }

        if (uniqueId != null) {
          processedElements.add(uniqueId);
        }
      }
    }
  }

  final outputFile = File('flutter_unified_catalog.json');
  outputFile.writeAsStringSync(jsonEncode(sdkData));
  print('\n🎯 Success! Unified JSON exported to ${outputFile.path}');
}

// ---------------------------------------------------------------------------
// HELPER: Web Scraper
// ---------------------------------------------------------------------------
Future<Map<String, Map<String, String>>> _fetchWebCategories() async {
  final client = HttpClient();
  final Map<String, Map<String, String>> categoryMap = {};

  try {
    final indexUrl = Uri.parse('https://docs.flutter.dev/ui/widgets');
    final indexRequest = await client.getUrl(indexUrl);
    final indexResponse = await indexRequest.close();

    if (indexResponse.statusCode != 200) return categoryMap;

    final indexHtml = await indexResponse.transform(utf8.decoder).join();
    final slugRegex = RegExp(
      r"""href=["'](?:https?:\/\/docs\.flutter\.dev)?\/ui\/widgets\/([a-z0-9-]+)\/?["']""",
    );
    final categorySlugs = slugRegex
        .allMatches(indexHtml)
        .map((m) => m.group(1)!)
        .where((slug) => slug != 'index' && slug.isNotEmpty)
        .toSet();

    for (final slug in categorySlugs) {
      final categoryUrl = Uri.parse(
        'https://docs.flutter.dev/ui/widgets/$slug',
      );
      final request = await client.getUrl(categoryUrl);
      final response = await request.close();

      if (response.statusCode != 200) continue;
      final html = await response.transform(utf8.decoder).join();

      final titleMatch = RegExp(
        r'<title>(.*?)(?:\s+widgets)?\s*\|\s*Flutter<\/title>',
        caseSensitive: false,
      ).firstMatch(html);
      String subCategory = titleMatch != null
          ? titleMatch.group(1)!.trim()
          : slug.replaceAll('-', ' ');

      String topLevel = "Base Widgets";
      if (slug.contains('cupertino') || slug.contains('material')) {
        topLevel = "Design Systems";
      }

      final widgetRegex = RegExp(r'\/([A-Za-z0-9_]+)-class\.html');
      for (final match in widgetRegex.allMatches(html)) {
        final widgetName = match.group(1)!;
        categoryMap.putIfAbsent(
          widgetName,
          () => {"top_level": topLevel, "sub_category": subCategory},
        );
      }
    }
  } catch (e) {
    print('⚠️ Web fetch failed: $e');
  } finally {
    client.close();
  }

  return categoryMap;
}

// ---------------------------------------------------------------------------
// HELPER: Extract Summary from AST Documentation
// ---------------------------------------------------------------------------
String _generateSummary(String? fullDoc) {
  if (fullDoc == null || fullDoc.isEmpty) return "No summary available.";

  // The analyzer strips the "///" but keeps newlines.
  // Usually, the first paragraph is separated by a double newline.
  final parts = fullDoc.trim().split('\n\n');
  if (parts.isEmpty) return "No summary available.";

  // Clean up the first paragraph to be a single continuous string
  return parts.first.replaceAll('\n', ' ').trim();
}

// ---------------------------------------------------------------------------
// AST EXTRACTION METHODS
// ---------------------------------------------------------------------------

Map<String, dynamic> _extractClassLike(InterfaceElement element) {
  final isWidget =
      element is ClassElement &&
      (element.allSupertypes.any((t) => t.element.name == 'Widget'));
  final rawDoc = element.documentationComment ?? '';

  return {
    'name': element.name,
    'summary': _generateSummary(rawDoc), // Clean, regex-free summary!
    'documentation': rawDoc,
    'isWidget': isWidget,
    'superChain': element.allSupertypes.map((t) => t.element.name).toList(),
    'constructors': element.constructors
        .map(
          (c) => {
            'name': (c.name?.isEmpty ?? false)
                ? element.name
                : '${element.name}.${c.name}',
            'documentation': c.documentationComment ?? '',
            'parameters': c.formalParameters.map(_extractParameter).toList(),
          },
        )
        .toList(),
    'properties': element.fields
        .where((f) => !(f.name?.startsWith('_') ?? false))
        .map(
          (f) => {
            'name': f.name,
            'type': f.type.getDisplayString(),
            'isFinal': f.isFinal,
            'isStatic': f.isStatic,
            'documentation': f.documentationComment ?? '',
          },
        )
        .toList(),
    'methods': element.methods
        .where((m) => !(m.name?.startsWith('_') ?? false))
        .map(
          (m) => {
            'name': m.name,
            'returnType': m.returnType.getDisplayString(),
            'isStatic': m.isStatic,
            'documentation': m.documentationComment ?? '',
            'parameters': m.formalParameters.map(_extractParameter).toList(),
          },
        )
        .toList(),
  };
}

Map<String, dynamic> _extractEnum(EnumElement element) {
  return {
    'name': element.name,
    'summary': _generateSummary(element.documentationComment),
    'documentation': element.documentationComment ?? '',
    'values': element.fields
        .where((f) => f.isEnumConstant)
        .map((f) => f.name)
        .toList(),
  };
}

Map<String, dynamic> _extractExtension(ExtensionElement element) {
  return {
    'name': element.name ?? 'AnonymousExtension',
    'summary': _generateSummary(element.documentationComment),
    'extendedType': element.extendedType.getDisplayString(),
    'documentation': element.documentationComment ?? '',
    'methods': element.methods
        .where((m) => !(m.name?.startsWith('_') ?? false))
        .map(
          (m) => {
            'name': m.name,
            'returnType': m.returnType.getDisplayString(),
            'documentation': m.documentationComment ?? '',
            'parameters': m.formalParameters.map(_extractParameter).toList(),
          },
        )
        .toList(),
  };
}

Map<String, dynamic> _extractExtensionType(ExtensionTypeElement element) {
  return {
    'name': element.name,
    'summary': _generateSummary(element.documentationComment),
    'documentation': element.documentationComment ?? '',
    'representationType': element.representation.type.getDisplayString(),
    'methods': element.methods
        .where((m) => !(m.name?.startsWith('_') ?? false))
        .map(
          (m) => {
            'name': m.name,
            'returnType': m.returnType.getDisplayString(),
            'documentation': m.documentationComment ?? '',
            'parameters': m.formalParameters.map(_extractParameter).toList(),
          },
        )
        .toList(),
  };
}

Map<String, dynamic> _extractFunction(TopLevelFunctionElement element) {
  return {
    'name': element.name,
    'summary': _generateSummary(element.documentationComment),
    'returnType': element.returnType.getDisplayString(),
    'documentation': element.documentationComment ?? '',
    'parameters': element.formalParameters.map(_extractParameter).toList(),
  };
}

Map<String, dynamic> _extractParameter(FormalParameterElement p) {
  return {
    'name': p.name,
    'type': p.type.getDisplayString(),
    'isRequired': p.isRequired,
    'isNamed': p.isNamed,
    'defaultValue': p.defaultValueCode ?? '',
  };
}
