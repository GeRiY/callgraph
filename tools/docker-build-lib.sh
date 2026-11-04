#!/usr/bin/env bash
# Közös Docker-build segéd a build-linux.sh / build-windows.sh-hoz.
# docker_build <image> <platform> <volume> <log> <parancs>: egy konténeres build, utána takarítás —
# a konténert a --rm törli; a cargo-volume-ot mindig, az image-et akkor törli, ha a build töltötte le (előtte nem volt meg).
# A buildek egymás után futnak (a hívó sorban hívja), hogy egyszerre csak egy image foglaljon helyet.

docker_build() {
    local image="$1" platform="$2" volume="$3" log="$4" cmd="$5"
    local had_image=0
    docker image inspect "$image" >/dev/null 2>&1 && had_image=1
    docker run --rm --platform "$platform" \
        -v "$ROOT:/src:ro" -v "$OUT:/out" -v "$volume:/usr/local/cargo/registry" -w /src \
        "$image" sh -c "$cmd" > "$log" 2>&1
    local rc=$?
    docker volume rm "$volume" >/dev/null 2>&1 || true
    if (( had_image == 0 )); then
        docker rmi "$image" >/dev/null 2>&1 || true
    fi
    return $rc
}
