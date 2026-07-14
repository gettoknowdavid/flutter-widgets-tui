import 'dart:convert';
import 'dart:io';

void main() async {
  final client = HttpClient();

  // This will hold our final, deduplicated list of widgets
  final Map<String, Map<String, dynamic>> masterCatalog = {};

  print(
    '🚀 Step 1: Dynamically discovering categories from docs.flutter.dev...',
  );

  // 1. Fetch the main widget index to discover all categories dynamically
  final indexUrl = Uri.parse('https://docs.flutter.dev/ui/widgets');
  final Set<String> categorySlugs = {};

  try {
    final indexRequest = await client.getUrl(indexUrl);
    final indexResponse = await indexRequest.close();
    if (indexResponse.statusCode == 200) {
      final indexHtml = await indexResponse.transform(utf8.decoder).join();

      // Match links that specifically route to widget categories
      final slugRegex = RegExp(
        r"""href=["'](?:https?:\/\/docs\.flutter\.dev)?\/ui\/widgets\/([a-z0-9-]+)\/?["']""",
      );
      final matches = slugRegex.allMatches(indexHtml);

      for (final match in matches) {
        final slug = match.group(1)!;
        // Ignore generic root paths if any get caught
        if (slug != 'index' && slug.isNotEmpty) {
          categorySlugs.add(slug);
        }
      }
    }
  } catch (e) {
    print('❌ Failed to fetch main index: $e');
    return;
  }

  print(
    '✅ Discovered ${categorySlugs.length} dynamic categories: ${categorySlugs.join(', ')}\n',
  );

  // 2. Fetch each discovered category page to populate our master roster
  for (final slug in categorySlugs) {
    final categoryUrl = Uri.parse('https://docs.flutter.dev/ui/widgets/$slug');
    try {
      final request = await client.getUrl(categoryUrl);
      final response = await request.close();

      if (response.statusCode != 200) continue;

      final html = await response.transform(utf8.decoder).join();

      // Dynamically get the clean category name from the <title> tag
      final titleMatch = RegExp(
        r'<title>(.*?)(?:\s+widgets)?\s*\|\s*Flutter<\/title>',
        caseSensitive: false,
      ).firstMatch(html);
      String subCategory = titleMatch != null
          ? titleMatch.group(1)!.trim()
          : slug.replaceAll('-', ' ');

      // Determine the Top Level category
      String topLevel = "Base Widgets";
      if (slug.contains('cupertino') || slug.contains('material')) {
        topLevel = "Design Systems";
      }

      // Extract every widget name linked on this category page
      final widgetRegex = RegExp(r'\/([A-Za-z0-9_]+)-class\.html');
      final widgetMatches = widgetRegex.allMatches(html);

      int count = 0;
      for (final match in widgetMatches) {
        final widgetName = match.group(1)!;

        // Add to roster. If it already exists, we keep the first found category.
        masterCatalog.putIfAbsent(
          widgetName,
          () => {
            "widget_name": widgetName,
            "top_level": topLevel,
            "sub_category": subCategory,
            "summary": "No summary available.", // Placeholder for Step 2
          },
        );
        count++;
      }
      print('   -> Added $count widgets from "$subCategory"');
    } catch (e) {
      print('❌ Failed to pull data for slug $slug: $e');
    }
  }

  client.close();
  print('\n📁 Step 2: Scanning local Flutter SDK for precise descriptions...');

  // 3. Point this to your local Flutter SDK lib directory
  final flutterLibPath = r'F:\sdk\flutter\packages\flutter\lib\src';
  final directory = Directory(flutterLibPath);

  if (!directory.existsSync()) {
    print('Directory not found: $flutterLibPath');
    print('Please check your path and try again.');
    return;
  }

  final files = directory
      .listSync(recursive: true)
      .whereType<File>()
      .where((f) => f.path.endsWith('.dart'));

  // Upgraded regex: Accounts for Generics (<T>) and captures what it extends
  final classDefRegex = RegExp(
    r'class\s+([A-Za-z0-9_]+)(?:<[^>]+>)?\s+(?:extends|implements)\s+([A-Za-z0-9_]+)',
  );

  for (var file in files) {
    final lines = file.readAsLinesSync();
    String currentSummary = "";

    for (var line in lines) {
      final trimmed = line.trim();

      // Capture Documentation Comments
      if (trimmed.startsWith('///')) {
        final docText = trimmed.replaceFirst('///', '').trim();
        if (docText.isNotEmpty &&
            !docText.startsWith('{@') &&
            currentSummary.isEmpty) {
          currentSummary = docText;
        }
      }
      // Ignore annotations
      else if (trimmed.startsWith('@')) {
        continue;
      }
      // Identify classes
      else if (trimmed.startsWith('class ')) {
        final match = classDefRegex.firstMatch(trimmed);

        if (match != null) {
          final className = match.group(1)!;
          final extendsName = match.group(2)!;

          final isKnownWidget = masterCatalog.containsKey(className);
          // Catch unlisted widgets hidden in the SDK just to be safe
          final isUndocumentedWidget = extendsName.endsWith('Widget');

          if (isKnownWidget || isUndocumentedWidget) {
            if (isKnownWidget) {
              // Apply summary to our officially tracked widget
              if (currentSummary.isNotEmpty &&
                  masterCatalog[className]!["summary"] ==
                      "No summary available.") {
                masterCatalog[className]!["summary"] = currentSummary;
              }
            } else {
              // It's an undocumented widget, add it to Miscellaneous
              String topLevel = "Base Widgets";
              String subCategory = "Miscellaneous";

              if (file.path.contains(r'\material\') ||
                  file.path.contains('/material/')) {
                topLevel = "Design Systems";
                subCategory = "Material Components";
              } else if (file.path.contains(r'\cupertino\') ||
                  file.path.contains('/cupertino/')) {
                topLevel = "Design Systems";
                subCategory = "Cupertino";
              }

              masterCatalog[className] = {
                "widget_name": className,
                "top_level": topLevel,
                "sub_category": subCategory,
                "summary": currentSummary.isNotEmpty
                    ? currentSummary
                    : "No summary available.",
              };
            }
          }
        }
        // Reset state for the next class
        currentSummary = "";
      }
      // Reset state if we hit normal code
      else if (trimmed.isNotEmpty) {
        currentSummary = "";
      }
    }
  }

  // 4. Output to JSON file
  final outputFile = File('catalog_data.json');
  final encoder = JsonEncoder.withIndent('  ');

  // Convert map to list for final JSON array
  final finalCatalogList = masterCatalog.values.toList();
  await outputFile.writeAsString(encoder.convert(finalCatalogList));

  print(
    '\n🎯 Success! Generated widget_catalog.json with ${finalCatalogList.length} total widgets.',
  );
}
