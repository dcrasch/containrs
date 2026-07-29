# Base Image

## Alpine

Download a base image filesystem from <https://alpinelinux.org/downloads/>
Choose the Minimal root file system for use in containers and minimal chroots.
Extract the compressed image and place it in a directory.

```bash
wget https://dl-cdn.alpinelinux.org/alpine/v3.24/releases/x86_64/alpine-minirootfs-3.24.1-x86_64.tar.gz
mkdir alpine
tar xvf alpine-minirootfs-3.24.1-x86_64.tar.gz -C alpine
```
