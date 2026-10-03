#include "file_reservation.h"
#include <fstream>
#include <iostream>
int main() {
    const std::string path = "reservation-fixture.bin";
    { std::ofstream file(path); file << "fixture"; }
    {
        ayther::runtime::FileReservation reserved(path, "");
        const auto writer = CreateFileA(path.c_str(), GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (writer != INVALID_HANDLE_VALUE) { CloseHandle(writer); return 1; }
        if (DeleteFileA(path.c_str())) return 2;
        if (MoveFileA(path.c_str(), "reservation-renamed.bin")) return 3;
    }
    bool mismatch{};
    try { ayther::runtime::FileReservation wrong(path, "wrong-revision"); }
    catch (const std::exception&) { mismatch = true; }
    if (!mismatch) return 4;
    const auto writer = CreateFileA(path.c_str(), GENERIC_WRITE, FILE_SHARE_READ,
        nullptr, OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
    if (writer == INVALID_HANDLE_VALUE) return 5;
    CloseHandle(writer);
    if (!DeleteFileA(path.c_str())) return 6;
    std::cout << "Runtime reservation blocks writes, deletion and rename; rejects another revision; releases on scope exit.\n";
}
