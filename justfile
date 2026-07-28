DATA_DIR := "."
ALPINE_VERSION := "3.21.3"
ALPINE_BRANCH := "v3.21"
ARCH := "x86_64"
alpine:
  mkdir -p {{DATA_DIR}}/remote-cache
  curl -L -o {{DATA_DIR}}/remote-cache/{{ALPINE_VERSION}}-{{ARCH}}.tar.gz "https://dl-cdn.alpinelinux.org/alpine/{{ALPINE_BRANCH}}/releases/{{ARCH}}/alpine-minirootfs-{{ALPINE_VERSION}}-{{ARCH}}.tar.gz"
build:
  cargo build
run: build
  sudo ./target/debug/containrs
