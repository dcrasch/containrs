# Unshare

Moves the calling process to a new namespace.

* mount namespaces
* uts namespace
* ipc namespaces
* TODO network namespace
* pid_namespaces
* TODO user namespace
* TODO cgroup namespace
* NOT USED time namespace

## CLONE_NEWNS

Creates an exact copy of the parent's mount table.
This is not shared shared with any other process.
Implies CLONE_FS.

## CLONE_FS (implied)

Unshare the file system attributes, the calling process no longer shares its root directory

## CLONE_NEWUTS

UTS namespaces provide isolation of two system identifiers: the hostname and the NIS domain name. 
Requires  CAP_SYS_ADMIN capability.
 
## CLONE_NEWPID

Unshare the PID namespace. The first child created by the calling process will assume the role of init process ID 1.
Requires  CAP_SYS_ADMIN capability.
 
## CLONE_NEWIPC

The calling process has a private copy of the IPC.  Isolates the System V IPC, Posix message queues.
Requires  CAP_SYS_ADMIN capability.
