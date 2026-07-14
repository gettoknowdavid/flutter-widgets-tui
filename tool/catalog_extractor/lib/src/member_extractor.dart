import 'package:analyzer/dart/element/element.dart';
import 'package:analyzer/dart/element/type.dart';

import 'doc_comment_parser.dart';
import 'model.dart';

/// `Object`-inherited noise every class picks up — not useful in a widget
/// reference catalog, filtered out of the inherited-methods walk below.
const Set<String> _objectNoiseMethods = {
  'toString',
  'hashCode',
  'noSuchMethod',
  'runtimeType',
  '==',
};

Object? _typeElement(DartType type) =>
    type is InterfaceType ? type.element : null;

ParameterRecord _toParameterRecord(FormalParameterElement p) {
  return ParameterRecord(
    name: p.name ?? '',
    type: p.type.getDisplayString(),
    isRequired: p.isRequired,
    isNamed: p.isNamed,
    defaultValue: p.defaultValueCode ?? '',
  );
}

List<ConstructorRecord> extractConstructors(InterfaceElement element) {
  final ownerName = element.name ?? '';
  return element.constructors.map((c) {
    final shortName = (c.name == null || c.name!.isEmpty)
        ? ownerName
        : '$ownerName.${c.name}';
    return ConstructorRecord(
      name: shortName,
      documentation: cleanShortDoc(c.documentationComment ?? ''),
      parameters: c.formalParameters.map(_toParameterRecord).toList(),
    );
  }).toList();
}

/// Finds the constructor to source property default values from: the
/// unnamed constructor if one exists, else the first declared constructor.
/// Flutter widgets overwhelmingly follow the `const Widget({ ... })`
/// unnamed-constructor convention, so this covers the common case;
/// widgets with only named constructors (e.g. some `.builder`-only types)
/// simply won't get default-value enrichment on their properties — an
/// accepted, documented gap rather than a silent one.
ConstructorElement? _primaryConstructor(InterfaceElement element) {
  if (element.constructors.isEmpty) return null;
  for (final c in element.constructors) {
    if (c.name == null || c.name!.isEmpty) return c;
  }
  return element.constructors.first;
}

/// Declared-only (not inherited) properties — matches the original
/// extractor's behavior and how Flutter's own widget docs present a
/// "properties" pane: a widget's *own* fields, not every field it
/// inherited from `Widget`/`StatelessWidget`/etc. (Contrast with
/// [extractMethods] below, which deliberately DOES walk the inheritance
/// chain — that asymmetry is intentional, not an oversight: methods like
/// `createElement()` are meaningfully "part of the API surface" even when
/// inherited, whereas inherited fields on widgets are essentially always
/// just `key`, which isn't worth repeating on every single entry.)
List<PropertyRecord> extractProperties(InterfaceElement element) {
  final primary = _primaryConstructor(element);
  final defaultsByName = <String, String>{};
  final requiredByName = <String, bool>{};
  if (primary != null) {
    for (final p in primary.formalParameters) {
      final name = p.name;
      if (name == null) continue;
      if ((p.defaultValueCode ?? '').isNotEmpty) {
        defaultsByName[name] = p.defaultValueCode!;
      }
      requiredByName[name] = p.isRequired;
    }
  }

  final result = <PropertyRecord>[];
  for (final f in element.fields) {
    // Replacement for the deprecated `FieldElement.isSynthetic`.
    //
    // analyzer 10.0.1 deprecated `FieldElement.isSynthetic` in favor of
    // `isOriginDeclaration` / `isOriginGetterSetter` /
    // `isOriginDeclaringFormalParameter` / `isOriginEnumValues`.
    // `isOriginDeclaration == true` is the "this came from an actual
    // `final`/`var` field declaration the author wrote" case — exactly
    // what `!isSynthetic` used to mean. The three alternatives cover the
    // synthetic cases we want to skip here: a compiler-synthesized backing
    // field for a getter/setter pair, a field implicitly introduced by a
    // constructor's declaring formal parameter, and enum-constant fields
    // (irrelevant here anyway — those only ever show up on `EnumElement`,
    // handled separately by `extractEnumValues`).
    //
    // Pinned against `analyzer: 13.3.0` per `pubspec.yaml` — same
    // re-verify-on-upgrade caveat as `widget_filter.dart`'s
    // `_hasRealSourceFile`.
    if (!f.isOriginDeclaration) continue;

    final name = f.name;
    if (name == null || name.startsWith('_')) continue;

    final typeName = f.type.getDisplayString();
    final typeElement = _typeElement(f.type);

    String inputKind;
    List<String>? enumOptions;
    if (typeName == 'bool' || typeName == 'bool?') {
      inputKind = 'bool';
    } else if (typeElement is EnumElement) {
      inputKind = 'enum';
      enumOptions = typeElement.fields
          .where((v) => v.isEnumConstant)
          .map((v) => v.name)
          .whereType<String>()
          .toList();
    } else if (typeName.contains('int') || typeName.contains('double')) {
      inputKind = 'number';
    } else {
      inputKind = 'text';
    }

    result.add(
      PropertyRecord(
        name: name,
        type: typeName,
        defaultValue: defaultsByName[name],
        description: cleanShortDoc(f.documentationComment ?? ''),
        isRequired: requiredByName[name] ?? false,
        isStatic: f.isStatic,
        isFinal: f.isFinal,
        inputKind: inputKind,
        enumOptions: enumOptions,
      ),
    );
  }
  return result;
}

/// Declared AND inherited methods — the fix for the original extractor's
/// gap #2 ("no inherited members"). `createElement()` etc. only show up
/// on `StatelessWidget`/`StatefulWidget`, not on the leaf widget class
/// itself, so a declared-only walk left every simple widget's methods
/// pane looking near-empty.
///
/// Deliberately uses the PUBLIC `allSupertypes` API rather than
/// analyzer's internal `InheritanceManager3`
/// (`package:analyzer/src/dart/element/inheritance_manager3.dart`), which
/// is not part of the stable public surface — importing from `analyzer`'s
/// `src/` tree is exactly the kind of instability this project's own
/// `dartdoc` version-pinning discipline is meant to avoid, just for a
/// different package. The tradeoff: override resolution here is a simple
/// "first (most-derived) declaration by name wins" rule, not full Dart
/// override/covariance resolution. That's sufficient for a documentation
/// catalog — it is not a substitute for the compiler's own view, and two
/// unrelated methods that happen to share a name across the chain (rare,
/// but possible with mixins) could resolve to the "wrong" one. Flagging
/// this explicitly rather than presenting it as fully correct.
List<MethodRecord> extractMethods(InterfaceElement element) {
  final seenNames = <String>{};
  final result = <MethodRecord>[];

  void collect(InterfaceElement owner, {required bool inherited}) {
    for (final m in owner.methods) {
      final name = m.name;
      if (name == null || name.startsWith('_')) continue;
      if (_objectNoiseMethods.contains(name)) continue;
      if (!seenNames.add(name)) continue; // most-derived declaration wins

      result.add(
        MethodRecord(
          name: name,
          returnType: m.returnType.getDisplayString(),
          isStatic: m.isStatic,
          kind: m.isStatic ? 'static' : 'instance',
          description: cleanShortDoc(m.documentationComment ?? ''),
          parameters: m.formalParameters.map(_toParameterRecord).toList(),
          declaredOn: owner.name ?? '',
          isInherited: inherited,
        ),
      );
    }
  }

  collect(element, inherited: false);
  for (final superType in element.allSupertypes) {
    collect(superType.element, inherited: true);
  }
  return result;
}

List<EnumValueRecord> extractEnumValues(EnumElement element) {
  return element.fields.where((f) => f.isEnumConstant).map((f) {
    return EnumValueRecord(
      name: f.name ?? '',
      documentation: cleanShortDoc(f.documentationComment ?? ''),
    );
  }).toList();
}

/// Best-effort `@Deprecated` detection.
///
/// NOTE — the single most version-sensitive line in this whole tool:
/// `element.metadata2.hasDeprecated` is the modern analyzer Element-model
/// API as of the analyzer version pinned in `pubspec.yaml`. [element] is
/// deliberately typed `dynamic` here rather than a specific analyzer
/// `Element` type: a dynamic member access isn't checked at *compile*
/// time, so if a future analyzer upgrade renames/moves this getter, this
/// function fails at the try/catch below (returning `false`, logged
/// nowhere) instead of breaking the build. That's a deliberate
/// availability-over-correctness tradeoff for a single cosmetic flag —
/// if you see deprecated widgets NOT flagged as such after an analyzer
/// bump, check the `Metadata`/`Metadata2` class in
/// `package:analyzer/dart/element/element.dart` for your installed
/// version and fix this function specifically; nothing else in this tool
/// depends on it.
bool isElementDeprecated(dynamic element) {
  try {
    return element.metadata2.hasDeprecated as bool;
  } catch (_) {
    return false;
  }
}
