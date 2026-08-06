
default:demo
run *args: build 
  sudo ./target/debug/containrs {{args}}
build:
  cargo build
demo: (run "images/alpine/" "/bin/sh")
