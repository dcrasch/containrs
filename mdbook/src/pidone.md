# Pid 1

When you fork/clone  a proces using CLONE_NEWPID flag, the child has 1 , the first process to boot.


## Get the current process id

```sh
readlink /proc/self
```


## Orphaned processes

Pid 1 is responsible for orphaned processes.


