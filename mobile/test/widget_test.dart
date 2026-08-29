import 'package:flutter_test/flutter_test.dart';
import 'package:provider/provider.dart';

import 'package:reunite_mobile/app.dart';
import 'package:reunite_mobile/services/mesh_service.dart';

void main() {
  testWidgets('ReUniteApp renders the chat tab by default', (WidgetTester tester) async {
    await tester.pumpWidget(
      ChangeNotifierProvider(
        create: (_) => MeshService()..init(),
        child: const ReUniteApp(),
      ),
    );
    await tester.pump();

    expect(find.text('Emergency Chat'), findsOneWidget);
    expect(find.text('GPS & Peers'), findsOneWidget);
    expect(find.text('Networks'), findsOneWidget);
  });
}
