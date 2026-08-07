# Overlayfs


## Lower

This is the base layer that is readonly and never modified

## Upper

This is the layer where we writes all the changes.

## Work

Staging area for renames, whiteouts and opaque markers. The state is created in the workdir and when created linked to the upperdir.


## Merged

This is the actual mount point and the fs that the container sees.

- second
- third


## Pivot root

## Documentation

* <https://www.kernel.org/doc/html/latest/filesystems/overlayfs.html>
