import 'dart:convert';
import 'dart:io';
import 'package:flutter/material.dart';
import 'package:flutter_map/flutter_map.dart';
import 'package:latlong2/latlong.dart';
import 'package:geolocator/geolocator.dart';
import '../theme/app_theme.dart';

class LocationPickerScreen extends StatefulWidget {
  final LatLng? initialLocation;

  const LocationPickerScreen({super.key, this.initialLocation});

  @override
  State<LocationPickerScreen> createState() => _LocationPickerScreenState();
}

class _LocationPickerScreenState extends State<LocationPickerScreen> {
  final MapController _mapController = MapController();
  LatLng _selectedLocation = const LatLng(0, 0);
  bool _isLoadingCurrent = false;
  
  // Search state
  final TextEditingController _searchController = TextEditingController();
  List<Map<String, dynamic>> _searchResults = [];
  bool _isSearching = false;
  bool _showSearchResults = false;

  @override
  void initState() {
    super.initState();
    _selectedLocation = widget.initialLocation ?? const LatLng(51.5074, -0.1278); // London default
    _searchController.addListener(() {
      if (mounted) setState(() {});
    });
    _getCurrentLocation(init: true);
  }

  @override
  void dispose() {
    _searchController.dispose();
    _mapController.dispose();
    super.dispose();
  }

  Future<void> _getCurrentLocation({bool init = false}) async {
    setState(() {
      _isLoadingCurrent = true;
    });
    try {
      bool serviceEnabled = await Geolocator.isLocationServiceEnabled();
      if (!serviceEnabled) {
        if (!init && mounted) {
          ScaffoldMessenger.of(context).showSnackBar(
            SnackBar(content: Text('Location services are disabled.')),
          );
        }
        return;
      }

      LocationPermission permission = await Geolocator.checkPermission();
      if (permission == LocationPermission.denied) {
        permission = await Geolocator.requestPermission();
        if (permission == LocationPermission.denied) {
          return;
        }
      }

      if (permission == LocationPermission.deniedForever) {
        return;
      }

      final position = await Geolocator.getCurrentPosition(
        locationSettings: const LocationSettings(
          accuracy: LocationAccuracy.high,
        ),
      );

      final currentLatLng = LatLng(position.latitude, position.longitude);
      setState(() {
        _selectedLocation = currentLatLng;
      });
      _mapController.move(currentLatLng, 15.0);
    } catch (e) {
      debugPrint("Error getting current location: $e");
    } finally {
      if (mounted) {
        setState(() {
          _isLoadingCurrent = false;
        });
      }
    }
  }

  Future<void> _searchLocation(String query) async {
    if (query.trim().isEmpty) return;
    setState(() {
      _isSearching = true;
      _showSearchResults = true;
    });
    
    final client = HttpClient();
    try {
      final uri = Uri.parse('https://nominatim.openstreetmap.org/search?format=json&q=${Uri.encodeComponent(query)}&limit=5');
      final request = await client.getUrl(uri);
      request.headers.set(HttpHeaders.userAgentHeader, 'IntrovertApp/1.0.0 (contact: support@introvert.chat)');
      final response = await request.close();
      
      if (response.statusCode == 200) {
        final responseBody = await response.transform(utf8.decoder).join();
        final List<dynamic> decoded = json.decode(responseBody);
        setState(() {
          _searchResults = decoded.map((e) => e as Map<String, dynamic>).toList();
        });
      } else {
        debugPrint("Nominatim API error: ${response.statusCode}");
      }
    } catch (e) {
      debugPrint("Error searching location: $e");
    } finally {
      client.close();
      if (mounted) {
        setState(() {
          _isSearching = false;
        });
      }
    }
  }

  void _selectSearchResult(Map<String, dynamic> result) {
    final lat = double.tryParse(result['lat']?.toString() ?? '');
    final lon = double.tryParse(result['lon']?.toString() ?? '');
    if (lat != null && lon != null) {
      final destination = LatLng(lat, lon);
      setState(() {
        _selectedLocation = destination;
        _showSearchResults = false;
        _searchResults.clear();
        _searchController.clear();
      });
      _mapController.move(destination, 15.0);
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: AppTheme.current.bg,
      appBar: AppBar(
        backgroundColor: AppTheme.current.surface,
        title: Text("Select Mesh Location", style: TextStyle(color: AppTheme.current.text, fontSize: 16, fontWeight: FontWeight.bold)),
        leading: IconButton(
          icon: Icon(Icons.arrow_back, color: AppTheme.current.text.withValues(alpha: 0.7)),
          onPressed: () => Navigator.pop(context),
        ),
      ),
      body: Stack(
        children: [
          Container(
            color: const Color(0xFFF5F5F5), // Light background behind map to avoid black grid during loading
            child: FlutterMap(
              mapController: _mapController,
              options: MapOptions(
                initialCenter: _selectedLocation,
                initialZoom: 15.0,
                onPositionChanged: (position, hasGesture) {
                  if (hasGesture) {
                    setState(() {
                      _selectedLocation = position.center;
                    });
                  }
                },
              ),
              children: [
                TileLayer(
                  urlTemplate: "https://{s}.basemaps.cartocdn.com/rastertiles/voyager/{z}/{x}/{y}.png",
                  subdomains: ['a', 'b', 'c', 'd'],
                  userAgentPackageName: 'chat.introvert.app',
                  fallbackUrl: "https://tile.openstreetmap.org/{z}/{x}/{y}.png",
                ),
                MarkerLayer(
                  markers: [
                    Marker(
                      point: _selectedLocation,
                      width: 80,
                      height: 80,
                      child: Icon(
                        Icons.location_on_rounded,
                        color: Colors.redAccent,
                        size: 40,
                      ),
                    ),
                  ],
                ),
              ],
            ),
          ),

          // Search Bar Overlay
          Positioned(
            top: 16,
            left: 16,
            right: 16,
            child: Column(
              children: [
                Container(
                  decoration: BoxDecoration(
                    color: AppTheme.current.surface,
                    borderRadius: BorderRadius.circular(30),
                    border: Border.all(color: AppTheme.current.accent.withValues(alpha: 0.3)),
                    boxShadow: [
                      BoxShadow(
                        color: Colors.black26,
                        blurRadius: 8,
                        offset: Offset(0, 4),
                      )
                    ],
                  ),
                  child: Row(
                    children: [
                      Padding(
                        padding: EdgeInsets.symmetric(horizontal: 16),
                        child: Icon(Icons.search, color: AppTheme.current.mutedText),
                      ),
                      Expanded(
                        child: TextField(
                          controller: _searchController,
                          style: TextStyle(color: AppTheme.current.text, fontSize: 14),
                          decoration: InputDecoration(
                            hintText: "Search city, street or landmark...",
                            hintStyle: TextStyle(color: AppTheme.current.text.withValues(alpha: 0.3), fontSize: 14),
                            border: InputBorder.none,
                          ),
                          onSubmitted: _searchLocation,
                        ),
                      ),
                      if (_searchController.text.isNotEmpty)
                        IconButton(
                          icon: Icon(Icons.close, color: AppTheme.current.mutedText),
                          onPressed: () {
                            setState(() {
                              _searchController.clear();
                              _searchResults.clear();
                              _showSearchResults = false;
                            });
                          },
                        ),
                    ],
                  ),
                ),
                if (_showSearchResults) ...[
                  SizedBox(height: 8),
                  Container(
                    constraints: const BoxConstraints(maxHeight: 250),
                    decoration: BoxDecoration(
                      color: AppTheme.current.surface,
                      borderRadius: BorderRadius.circular(16),
                      border: Border.all(color: AppTheme.current.mutedText.withValues(alpha: 0.1)),
                    ),
                    child: _isSearching
                        ? Padding(
                            padding: EdgeInsets.all(16.0),
                            child: Center(
                              child: SizedBox(
                                width: 24,
                                height: 24,
                                child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent),
                              ),
                            ),
                          )
                        : _searchResults.isEmpty
                            ? Padding(
                                padding: EdgeInsets.all(16.0),
                                child: Center(
                                  child: Text("No location found.", style: TextStyle(color: AppTheme.current.mutedText, fontSize: 13)),
                                ),
                              )
                            : ListView.separated(
                                shrinkWrap: true,
                                padding: EdgeInsets.symmetric(vertical: 8),
                                itemCount: _searchResults.length,
                                separatorBuilder: (context, index) => Divider(color: AppTheme.current.mutedText.withValues(alpha: 0.1), height: 1),
                                itemBuilder: (context, index) {
                                  final res = _searchResults[index];
                                  final name = res['display_name'] ?? 'Unknown location';
                                  return ListTile(
                                    leading: Icon(Icons.location_on_outlined, color: AppTheme.current.accent),
                                    title: Text(
                                      name,
                                      style: TextStyle(color: AppTheme.current.text, fontSize: 13),
                                      maxLines: 2,
                                      overflow: TextOverflow.ellipsis,
                                    ),
                                    onTap: () => _selectSearchResult(res),
                                  );
                                },
                              ),
                  ),
                ],
              ],
            ),
          ),

          // Bottom Action Panel
          Positioned(
            bottom: 24,
            left: 16,
            right: 16,
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.end,
              children: [
                FloatingActionButton(
                  backgroundColor: AppTheme.current.surface,
                  foregroundColor: AppTheme.current.accent,
                  shape: RoundedRectangleBorder(
                    borderRadius: BorderRadius.circular(30),
                    side: BorderSide(color: AppTheme.current.accent, width: 1),
                  ),
                  onPressed: () => _getCurrentLocation(),
                  child: _isLoadingCurrent
                      ? SizedBox(
                          width: 20,
                          height: 20,
                          child: CircularProgressIndicator(strokeWidth: 2, color: AppTheme.current.accent),
                        )
                      : Icon(Icons.my_location),
                ),
                SizedBox(height: 16),
                Container(
                  padding: EdgeInsets.all(16),
                  decoration: BoxDecoration(
                    color: AppTheme.current.surface,
                    borderRadius: BorderRadius.circular(20),
                    border: Border.all(color: Colors.redAccent.withValues(alpha: 0.3)),
                    boxShadow: [
                      BoxShadow(
                        color: Colors.black45,
                        blurRadius: 10,
                        offset: Offset(0, 4),
                      )
                    ],
                  ),
                  child: Column(
                    mainAxisSize: MainAxisSize.min,
                    children: [
                      Row(
                        children: [
                          Icon(Icons.map_outlined, color: Colors.redAccent),
                          SizedBox(width: 12),
                          Expanded(
                            child: Column(
                              crossAxisAlignment: CrossAxisAlignment.start,
                              children: [
                                Text("SELECTED COORDINATES", style: TextStyle(color: Colors.redAccent, fontSize: 10, fontWeight: FontWeight.bold, letterSpacing: 1.2)),
                                SizedBox(height: 4),
                                Text(
                                  "Lat: ${_selectedLocation.latitude.toStringAsFixed(6)}, Lng: ${_selectedLocation.longitude.toStringAsFixed(6)}",
                                  style: TextStyle(color: AppTheme.current.text, fontSize: 12, fontFamily: 'monospace'),
                                ),
                              ],
                            ),
                          ),
                        ],
                      ),
                      SizedBox(height: 16),
                      Row(
                        children: [
                          Expanded(
                            child: ElevatedButton.icon(
                              icon: Icon(Icons.send_rounded, size: 16, color: Colors.black),
                              label: Text("SEND PIN LOCATION", style: TextStyle(color: Colors.black, fontWeight: FontWeight.bold, fontSize: 13)),
                              style: ElevatedButton.styleFrom(
                                backgroundColor: Colors.redAccent,
                                shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(12)),
                                padding: EdgeInsets.symmetric(vertical: 14),
                              ),
                              onPressed: () {
                                Navigator.pop(context, _selectedLocation);
                              },
                            ),
                          ),
                        ],
                      ),
                    ],
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
  }
}
