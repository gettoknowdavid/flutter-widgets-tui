import 'dart:convert';
import 'dart:io';
import 'package:analyzer/dart/analysis/analysis_context_collection.dart';
import 'package:analyzer/dart/analysis/results.dart';
import 'package:analyzer/dart/element/element.dart';
import 'package:path/path.dart' as p;

void main(List<String> args) async {
  if (args.isEmpty) {
    print('Usage: dart run catalog_extractor_new.dart <PATH_TO_FLUTTER_SDK>');
    return;
  }

  final flutterSdkPath = args[0];
  // Pointing to the package root where pubspec.yaml lives
  final flutterPackagePath = p.normalize(
    p.absolute(p.join(flutterSdkPath, 'packages', 'flutter')),
  );
  final libPath = p.join(flutterPackagePath, 'lib');

  print('🚀 Analyzing Flutter SDK at: $flutterPackagePath');

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

  // 1. Get all top-level .dart files in /lib
  final topLevelFiles = Directory(
    libPath,
  ).listSync().whereType<File>().where((f) => f.path.endsWith('.dart'));

  for (final file in topLevelFiles) {
    print('🔍 Analyzing entry point: ${p.basename(file.path)}');

    final result = await session.getResolvedLibrary(file.path);
    if (result is ResolvedLibraryResult) {
      final exportNamespace = result.element.exportNamespace;

      for (final element in exportNamespace.definedNames2.values) {
        // Filter out private members
        if (element.name?.startsWith('_') ?? true) continue;

        // --- THE FIX ---
        // Safely extract the URI, falling back through available source getters
        final sourceUri = element.library?.uri.toString() ?? '';

        // 1. Reject standard Dart libraries (e.g., dart:ui, dart:core)
        if (sourceUri.startsWith('dart:')) continue;

        // 2. If it resolved as a package URI, ensure it is exactly package:flutter
        if (sourceUri.startsWith('package:') &&
            !sourceUri.startsWith('package:flutter/')) {
          continue;
        }

        // 3. If it resolved as a local file URI, ensure it belongs to the flutter package's lib directory
        if (sourceUri.startsWith('file:') &&
            !sourceUri.contains('/packages/flutter/lib/')) {
          continue;
        }
        // ---------------

        String? uniqueId;
        if (element is ClassElement) {
          uniqueId = 'class:${element.name}';
          if (!processedElements.contains(uniqueId)) {
            sdkData['classes'].add(_extractClassLike(element));
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
    } else {
      print('⚠️ Warning: Could not fully resolve library ${file.path}');
    }
  }

  File('widgets_data.json').writeAsStringSync(jsonEncode(sdkData));
  print(
    '🎯 Success! Extracted ${processedElements.length} elements to widgets_data.json',
  );
}

// --- Data Extraction Methods (Upgraded for latest AST Models) ---
// Keep your existing _extractClassLike, _extractEnum, etc. methods here exactly as they are.

Map<String, dynamic> _extractClassLike(InterfaceElement element) {
  final isWidget =
      element is ClassElement &&
      (element.allSupertypes.any((t) => t.element.name == 'Widget'));

  return {
    'name': element.name,
    'documentation': element.documentationComment ?? '',
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
        .where(
          // Swapped isOriginEnumValues for isSynthetic to cleanly ignore auto-generated properties
          (f) => !(f.name?.startsWith('_') ?? false),
        )
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
