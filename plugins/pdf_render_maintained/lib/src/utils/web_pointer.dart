import 'dart:typed_data';

/// [Uint8] is not defined for Flutter Web. This is just a dummy definition.
class Uint8 {}

/// Works only for [Uint8];
class Pointer<T> {
  final int _address;
  Pointer._(this._address);
  factory Pointer.fromAddress(int address) => Pointer._(address);

  int get address => _address;
  Uint8List get buffer => getBufferByFakeAddress(address);
}

/// Get buffer for `Pointer<Uint8>`.
extension Uint8Pointer on Pointer<Uint8> {
  Uint8List asTypedList(int length) => buffer;
}

int _fakeAddress = 0;

// Storage for buffers using a simple Map
final Map<int, Uint8List> _bufferStorage = {};

/// Associate an address with the specified buffer and return the address.
int pinBufferByFakeAddress(Uint8List buffer) {
  final address = _fakeAddress++;
  _bufferStorage[address] = buffer;
  return address;
}

/// Get the associated buffer for the address.
Uint8List getBufferByFakeAddress(int address) {
  return _bufferStorage[address]!;
}

/// Release the associated buffer for the address.
void unpinBufferByFakeAddress(int address) {
  _bufferStorage.remove(address);
}
