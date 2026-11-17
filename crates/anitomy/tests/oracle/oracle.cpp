// Reference output from the C++ Anitomy: one JSON array per input line.
#include <cstdio>
#include <fstream>
#include <iostream>
#include <string>
#include <anitomy.hpp>
#include <anitomy/detail/format.hpp>

static std::string escape(const std::string& s) {
  std::string out;
  for (unsigned char c : s) {
    if (c == '"' || c == '\\') {
      out += '\\';
      out += static_cast<char>(c);
    } else if (c < 0x20) {
      char buf[8];
      std::snprintf(buf, sizeof buf, "\\u%04x", c);
      out += buf;
    } else {
      out += static_cast<char>(c);
    }
  }
  return out;
}

int main(int argc, char* argv[]) {
  if (argc < 3) return 1;
  std::ifstream in{argv[1], std::ios::binary};
  std::ofstream out{argv[2], std::ios::binary};
  std::string line;
  while (std::getline(in, line)) {
    if (!line.empty() && line.back() == '\r') line.pop_back();
    out << "[";
    bool first = true;
    for (const auto& e : anitomy::parse(line)) {
      if (!first) out << ",";
      first = false;
      out << "[\"" << anitomy::detail::to_string(e.kind) << "\",\"" << escape(e.value) << "\","
          << e.position << "]";
    }
    out << "]\n";
  }
  return 0;
}
