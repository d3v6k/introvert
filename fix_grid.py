with open('lib/views/theme_mockup_grid.dart', 'r') as f:
    c = f.read()
# Revert that bad block
c = c.replace("""        return Scaffold(
          backgroundColor: Colors.black,
          body: GridView.builder(
            padding: const EdgeInsets.all(16),
            gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
              crossAxisCount: columns,
              childAspectRatio: 0.55, // Typical phone aspect ratio
              crossAxisSpacing: 16,
              mainAxisSpacing: 16,
            ),
            itemCount: mockupThemes.length,
            itemBuilder: (context, index) {
              return Material(
                color: Colors.transparent,
                child: MockupAppShell(theme: mockupThemes[index])
              );
            },
          ),
        );
      },
    );
  }
}""", """          return GridView.builder(
            padding: const EdgeInsets.all(16),
            gridDelegate: SliverGridDelegateWithFixedCrossAxisCount(
              crossAxisCount: columns,
              childAspectRatio: 0.55, // Typical phone aspect ratio
              crossAxisSpacing: 16,
              mainAxisSpacing: 16,
            ),
            itemCount: mockupThemes.length,
            itemBuilder: (context, index) {
              return Material(
                color: Colors.transparent,
                child: MockupAppShell(theme: mockupThemes[index]),
              );
            },
          );
        },
      ),
    );
  }
}""")
with open('lib/views/theme_mockup_grid.dart', 'w') as f:
    f.write(c)
