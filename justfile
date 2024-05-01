target := `rustc -vV | sed -n 's|host: ||p'`
os_family := os_family()
archive_type := if os_family == "windows" { "zip" } else { "tarball" }
package_type := "none"
os := os()
arch := arch()
version := `toml get Cargo.toml package.version --raw`
archive_name := "bbpipelinewait-" + version + "-" + target
msg := "Unknown error"
binary_name := if os_family == "windows" { "bbpipelinewait.exe" } else { "bbpipelinewait " }
notes := "release"
extra_packages := ""

default: build

die:
    @echo "Error: {{ msg }}"

assert-darwin-host:
    @{{ if os == "macos" { "true" } else { "just msg=\"Not a darwin host\" die" } }}

clean:
    rm -rf target dist

build:
    cargo build --release --target {{ target }}

build-all: build-mac-m1 build-mac-x86 build-linux-amd64 build-linux-arm64

build-windows:
    just target=x86_64-pc-windows-msvc build

#	just target=aarch64-pc-windows-msvc build

build-mac-m1:
    just target=aarch64-apple-darwin assert-darwin-host archive

build-mac-x86:
    just target=x86_64-apple-darwin assert-darwin-host archive

build-linux-amd64:
    docker run --rm --platform linux/amd64 --user "$(id -u)":"$(id -g)" -v "$PWD":/usr/src/myapp -w /usr/src/myapp \
        rust:1.70.0 sh -c "cargo install just toml-cli && just archive"

build-linux-arm64:
    docker run --rm --platform linux/arm64 --user "$(id -u)":"$(id -g)" -v "$PWD":/usr/src/myapp -w /usr/src/myapp \
        rust:1.70.0 sh -c "cargo install just toml-cli && just archive"

archive-tarball:
    mkdir -p dist/{{ target }}
    tar czf dist/{{ target }}/{{ archive_name }}.tar.gz -C target/{{ target }}/release/ {{ binary_name }}

archive-zip:
    mkdir -p dist/{{ target }}
    zip dist/{{ target }}/{{ archive_name }}.zip target/{{ target }}/release/{{ binary_name }}

archive: build
    just target={{ target }} archive-{{ archive_type }}

archive-windows:
    just target=x86_64-pc-windows-msvc archive

#    just target=aarch64-pc-windows-msvc archive

archive-all:
    just target=aarch64-apple-darwin archive-tarball
    just target=x86_64-apple-darwin archive-tarball
    just target=aarch64-unknown-linux-gnu archive-tarball
    just target=x86_64-unknown-linux-gnu archive-tarball
    just target=x86_64-pc-windows-msvc binary_name=bbpipelinewait.exe archive-zip

#    just target=aarch64-pc-windows-msvc binary_name=bbpipelinewait.exe archive-zip

package:
    #!/usr/bin/env bash
    mkdir -p dist/{{ target }}
    source build/{{ target }}.env
    nfpm package -p {{ package_type }} \
      -f <(VERSION={{ version }} BIND_FILE=/usr/bin/bbpipelinewait envsubst < build/nfpm.yaml.tmpl) --target dist/{{ target }}/

package-termux:
    #!/usr/bin/env bash
    mkdir -p dist/{{ target }}
    mkdir -p dist/tmp/
    source build/{{ target }}.env
    nfpm package -p {{ package_type }} \
      -f <(VERSION={{ version }} BIND_FILE=/data/data/com.termux/files envsubst < build/nfpm.yaml.tmpl) \
      --target dist/tmp/
    export DEBARCH="{{ if target =~ '^x86_64' { "amd64" } else { "arm64" } }}"
    mv dist/tmp/bbpipelinewait_{{ version }}_${DEBARCH}.deb dist/{{ target }}/bbpipelinewait_{{ version }}_${DEBARCH}.termux.deb
    rmdir dist/tmp

linux-packages:
    just target=x86_64-unknown-linux-gnu package_type=deb package
    just target=x86_64-unknown-linux-gnu package_type=rpm package
    just target=x86_64-unknown-linux-gnu package_type=deb package-termux
    just target=aarch64-unknown-linux-gnu package_type=deb package
    just target=aarch64-unknown-linux-gnu package_type=rpm package
    just target=aarch64-unknown-linux-gnu package_type=deb package-termux

create-release: clobber-release
    gh release create {{ version }} --notes "{{ notes }}"

clobber-release:
    gh release delete {{ version }} -y || true

upload-to-release:
    gh release view {{ version }} || just msg="Release does not exist" die
    gh release upload {{ version }} \
        dist/aarch64-apple-darwin/bbpipelinewait-{{ version }}-aarch64-apple-darwin.tar.gz \
        dist/x86_64-apple-darwin/bbpipelinewait-{{ version }}-x86_64-apple-darwin.tar.gz \
        dist/aarch64-unknown-linux-gnu/bbpipelinewait-{{ version }}-aarch64-unknown-linux-gnu.tar.gz \
        dist/aarch64-unknown-linux-gnu/bbpipelinewait-{{ version }}-1.aarch64.rpm \
        dist/aarch64-unknown-linux-gnu/bbpipelinewait_{{ version }}_arm64.deb \
        dist/aarch64-unknown-linux-gnu/bbpipelinewait_{{ version }}_arm64.termux.deb \
        dist/x86_64-unknown-linux-gnu/bbpipelinewait-{{ version }}-x86_64-unknown-linux-gnu.tar.gz \
        dist/x86_64-unknown-linux-gnu/bbpipelinewait-{{ version }}-1.x86_64.rpm \
        dist/x86_64-unknown-linux-gnu/bbpipelinewait_{{ version }}_amd64.deb \
        dist/x86_64-unknown-linux-gnu/bbpipelinewait_{{ version }}_amd64.termux.deb \
        {{ extra_packages }} \
        --clobber

upload-to-release-ci:
    just extra_packages="dist/x86_64-pc-windows-msvc/bbpipelinewait-{{ version }}-x86_64-pc-windows-msvc.zip" \
    upload-to-release

#    just extra_packages="dist/x86_64-pc-windows-msvc/bbpipelinewait-{{version}}-x86_64-pc-windows-msvc.zip \
#    dist/aarch64-pc-windows-msvc/bbpipelinewait-{{version}}-aarch64-pc-windows-msvc.zip" upload-to-release

homebrew-program:
    #!/usr/bin/env bash
    export VERSION={{ version }}
    export X86_DARWIN_RELEASE_URL=$(gh release view {{ version }} --json assets --jq \
      '.assets[] | select(.name=="bbpipelinewait-{{ version }}-x86_64-apple-darwin.tar.gz") | .url')
    export X86_DARWIN_RELEASE_SHA=$(shasum -a 256 \
      dist/x86_64-apple-darwin/bbpipelinewait-{{ version }}-x86_64-apple-darwin.tar.gz | awk '{print $1}')
    export ARM64_DARWIN_RELEASE_URL=$(gh release view {{ version }} --json assets --jq \
      '.assets[] | select(.name=="bbpipelinewait-{{ version }}-aarch64-apple-darwin.tar.gz") | .url')
    export ARM64_DARWIN_RELEASE_SHA=$(shasum -a 256 \
      dist/aarch64-apple-darwin/bbpipelinewait-{{ version }}-aarch64-apple-darwin.tar.gz | awk '{print $1}')
    export X86_LINUX_RELEASE_URL=$(gh release view {{ version }} --json assets --jq \
      '.assets[] | select(.name=="bbpipelinewait-{{ version }}-x86_64-unknown-linux-gnu.tar.gz") | .url')
    export X86_LINUX_RELEASE_SHA=$(shasum -a 256 \
      dist/x86_64-unknown-linux-gnu/bbpipelinewait-{{ version }}-x86_64-unknown-linux-gnu.tar.gz | awk '{print $1}')
    export ARM64_LINUX_RELEASE_URL=$(gh release view {{ version }} --json assets --jq \
      '.assets[] | select(.name=="bbpipelinewait-{{ version }}-aarch64-unknown-linux-gnu.tar.gz") | .url')
    export ARM64_LINUX_RELEASE_SHA=$(shasum -a 256 \
      dist/aarch64-unknown-linux-gnu/bbpipelinewait-{{ version }}-aarch64-unknown-linux-gnu.tar.gz | awk '{print $1}')
    envsubst < build/program.rb.tmpl > dist/program.rb

homebrew-update: homebrew-program
    #!/usr/bin/env bash
    git clone git@github.com:scottmmjackson/homebrew-sj dist/tap
    cd dist/tap
    git checkout -b bbpipelinewait-{{ version }}
    cp ../program.rb Formula/bbpipelinewait.rb
    git add Formula/bbpipelinewait.rb
    git commit -m "Added formula for bbpipelinewait {{ version }}"
    git push origin HEAD --force # force push because this could be a second run
    gh pr create --title "Added formula for bbpipelinewait {{ version }}" --body "Added formula for bbpipelinewait {{ version }}"

do-release-build: build-all

do-release-package-preflight:
    @stat dist/aarch64-apple-darwin/bbpipelinewait-{{ version }}-aarch64-apple-darwin.tar.gz > /dev/null
    @stat dist/x86_64-apple-darwin/bbpipelinewait-{{ version }}-x86_64-apple-darwin.tar.gz > /dev/null
    @stat dist/aarch64-unknown-linux-gnu/bbpipelinewait-{{ version }}-aarch64-unknown-linux-gnu.tar.gz > /dev/null
    @stat dist/x86_64-unknown-linux-gnu/bbpipelinewait-{{ version }}-x86_64-unknown-linux-gnu.tar.gz > /dev/null
    # For linux packages
    @stat target/x86_64-unknown-linux-gnu/release/bbpipelinewait > /dev/null
    @stat target/aarch64-unknown-linux-gnu/release/bbpipelinewait > /dev/null

do-release-package-preflight-ci: do-release-package-preflight
    @stat dist/x86_64-pc-windows-msvc/bbpipelinewait-{{ version }}-x86_64-pc-windows-msvc.zip  > /dev/null

#    @stat dist/aarch64-pc-windows-msvc/bbpipelinewait-{{version}}-aarch64-pc-windows-msvc.zip > /dev/null

do-release-package: do-release-package-preflight linux-packages create-release upload-to-release homebrew-update

do-release-package-ci: do-release-package-preflight-ci linux-packages create-release upload-to-release-ci homebrew-update

do-release: do-release-build do-release-package
