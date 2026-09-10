{ stdenv
, cmake
, catch2_3
}:

stdenv.mkDerivation {
  pname = "libbbf-cpp";
  version = "3.0.0";
  src = builtins.fetchGit {
    url = "https://github.com/ef1500/libbbf.git";
    rev = "7fe8970aa093f288f26e0b5a4a0744d6ea778d1e";
    shallow = true;
  };

  nativeBuildInputs = [ cmake catch2_3 ];

  postPatch = ''
    substituteInPlace CMakeLists.txt --replace-fail "-Werror" ""
    patch -p1 < ${../patches/libbbf-bbfbench-no-shallow-copies.patch}
  '';

  cmakeFlags = [ "-DCMAKE_BUILD_TYPE=Release" ];

  installPhase = ''
    runHook preInstall
    cmake --install . --prefix $out
    bench=$(find .. -type f -name bbfbench -perm -u+x -print -quit)
    if [ -n "$bench" ]; then
      install -Dm755 "$bench" $out/bin/bbfbench
    fi
    runHook postInstall
  '';
}
