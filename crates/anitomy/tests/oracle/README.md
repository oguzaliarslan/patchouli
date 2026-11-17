# Golden data from the C++ Anitomy

`../data/golden.jsonl` was recorded by running the original C++ library
(erengy/anitomy at the commit Taiga's `develop` branch uses, 8498b53) over
`inputs.txt`: Anitomy's own `test/data.json` inputs plus extra edge cases.

To regenerate it, build `oracle.cpp` against Anitomy's `include` folder with a
C++23 compiler, for example with Visual Studio 2022 (in a "x64 Native Tools"
prompt):

```
cl /nologo /std:c++latest /EHsc /O2 /utf-8 /I <anitomy>\include oracle.cpp
oracle.exe inputs.txt golden.txt
```

Then combine each input line with the matching output line into
`{"input": ..., "elements": [...]}` objects, one per line.

To add a case, append the file name to `inputs.txt` and regenerate. Don't edit
expected output by hand: it must come from the C++ library.
